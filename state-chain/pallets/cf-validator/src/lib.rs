// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod mock;
mod tests;

mod helpers;

pub mod weights;
pub use weights::WeightInfo;
pub mod migrations;

mod auction_resolver;
mod benchmarking;
mod delegation;
mod rotation_state;

pub use auction_resolver::*;
pub use delegation::*;

use cf_primitives::{
	AccountRole, AuthorityCount, CfeCompatibility, Ed25519PublicKey, EpochIndex, Ipv6Addr, SemVer,
	DEFAULT_MAX_AUTHORITY_SET_CONTRACTION, FLIPPERINOS_PER_FLIP,
};
use cf_traits::{
	impl_pallet_safe_mode, offence_reporting::OffenceReporter, AccountInfo, AsyncResult,
	AuthoritiesCfeVersions, Bid, Bonding, CfePeerRegistration, Chainflip, EpochInfo,
	EpochTransitionHandler, ExecutionCondition, FundingInfo, HistoricalEpoch, KeyRotator,
	MissedAuthorshipSlots, OnAccountFunded, QualifyNode, RedemptionCheck, ReputationResetter,
	SetSafeMode,
};
use cf_utilities::Port;
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{BlockNumberProvider, One, Saturating, UniqueSaturatedInto, Zero},
		Percent, Permill,
	},
	traits::{EstimateNextSessionRotation, OnKilledAccount},
};
use frame_system::pallet_prelude::*;
use nanorand::{Rng, WyRand};
pub use pallet::*;
use sp_core::ed25519;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
};

use crate::rotation_state::RotationState;

type SessionIndex = u32;

type Version = SemVer;

type Ed25519Signature = ed25519::Signature;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletConfigUpdate {
	RegistrationBondPercentage {
		percentage: Percent,
	},
	AuctionBidCutoffPercentage {
		percentage: Percent,
	},
	RedemptionPeriodAsPercentage {
		percentage: Percent,
	},
	BackupRewardNodePercentage {
		percentage: Percent,
	},
	EpochDuration {
		blocks: u32,
	},
	AuthoritySetMinSize {
		min_size: AuthorityCount,
	},
	AuctionParameters {
		parameters: SetSizeParameters,
	},
	MinimumReportedCfeVersion {
		version: SemVer,
	},
	MaxAuthoritySetContractionPercentage {
		percentage: Percent,
	},
	/// Note the `minimum_flip_bid` is in whole FLIP, not flipperinos.
	MinimumAuctionBid {
		minimum_flip_bid: u32,
	},
}

type RuntimeRotationState<T> =
	RotationState<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(5);

// Might be better to add the enum inside a struct rather than struct inside enum
#[derive(Clone, PartialEq, Eq, Default, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum RotationPhase<T: Config> {
	#[default]
	Idle,
	KeygensInProgress(RuntimeRotationState<T>),
	KeyHandoversInProgress(RuntimeRotationState<T>),
	ActivatingKeys(RuntimeRotationState<T>),
	NewKeysActivated(RuntimeRotationState<T>),
	SessionRotating(Vec<ValidatorIdOf<T>>, <T as Chainflip>::Amount),
}

impl<T: pallet::Config> RotationPhase<T> {
	pub fn to_str(&self) -> &'static str {
		match self {
			RotationPhase::Idle => "Idle",
			RotationPhase::KeygensInProgress(_) => "KeygensInProgress",
			RotationPhase::KeyHandoversInProgress(_) => "KeyHandoversInProgress",
			RotationPhase::ActivatingKeys(_) => "ActivatingKeys",
			RotationPhase::NewKeysActivated(_) => "NewKeysActivated",
			RotationPhase::SessionRotating(_, _) => "SessionRotating",
		}
	}
}
type ValidatorIdOf<T> = <T as Chainflip>::ValidatorId;

type BackupMap<T> = BTreeMap<ValidatorIdOf<T>, <T as Chainflip>::Amount>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	MissedAuthorshipSlot,
}

impl_pallet_safe_mode!(PalletSafeMode; authority_rotation_enabled, start_bidding_enabled, stop_bidding_enabled);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{AccountRoleRegistry, KeyRotationStatusOuter, RotationBroadcastsPending};
	use frame_support::sp_runtime::app_crypto::RuntimePublic;
	use pallet_session::WeightInfo as SessionWeightInfo;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config + Chainflip + pallet_session::Config<ValidatorId = ValidatorIdOf<Self>>
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The top-level offence type must support this pallet's offence type.
		type Offence: From<PalletOffence>;

		/// A handler for epoch lifecycle events
		type EpochTransitionHandler: EpochTransitionHandler;

		type KeyRotator: KeyRotator<ValidatorId = ValidatorIdOf<Self>>;

		/// checks if there are any rotation txs pending from the last rotation
		type RotationBroadcastsPending: RotationBroadcastsPending;

		/// For retrieving missed authorship slots.
		type MissedAuthorshipSlots: MissedAuthorshipSlots;

		/// Criteria that need to be fulfilled to qualify as a validator node (authority or backup).
		type KeygenQualification: QualifyNode<<Self as Chainflip>::ValidatorId>;

		/// For reporting missed authorship slots.
		type OffenceReporter: OffenceReporter<
			ValidatorId = ValidatorIdOf<Self>,
			Offence = Self::Offence,
		>;

		/// Updates the bond of an authority.
		type Bonder: Bonding<AccountId = ValidatorIdOf<Self>, Amount = Self::Amount>;

		/// This is used to reset the validator's reputation
		type ReputationResetter: ReputationResetter<ValidatorId = ValidatorIdOf<Self>>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode> + SetSafeMode<PalletSafeMode>;

		type CfePeerRegistration: CfePeerRegistration<Self>;

		/// Benchmark weights.
		type ValidatorWeightInfo: WeightInfo;
	}

	/// Percentage of epoch we allow redemptions.
	#[pallet::storage]
	#[pallet::getter(fn redemption_period_as_percentage)]
	pub type RedemptionPeriodAsPercentage<T: Config> = StorageValue<_, Percent, ValueQuery>;

	/// The starting block number for the current epoch.
	#[pallet::storage]
	#[pallet::getter(fn current_epoch_started_at)]
	pub type CurrentEpochStartedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// The duration of an epoch in blocks.
	#[pallet::storage]
	#[pallet::getter(fn epoch_duration)]
	pub type EpochDuration<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Current epoch index.
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub type CurrentEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// Defines a unique index for each authority for each epoch.
	#[pallet::storage]
	#[pallet::getter(fn authority_index)]
	pub type AuthorityIndex<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		EpochIndex,
		Blake2_128Concat,
		ValidatorIdOf<T>,
		AuthorityCount,
	>;

	/// The rotation phase we are currently at.
	#[pallet::storage]
	#[pallet::getter(fn current_rotation_phase)]
	pub type CurrentRotationPhase<T: Config> = StorageValue<_, RotationPhase<T>, ValueQuery>;

	/// A set of the current authorities.
	#[pallet::storage]
	pub type CurrentAuthorities<T: Config> = StorageValue<_, Vec<ValidatorIdOf<T>>, ValueQuery>;

	/// The bond of the current epoch.
	#[pallet::storage]
	#[pallet::getter(fn bond)]
	pub type Bond<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// Account to Peer Mapping.
	#[pallet::storage]
	#[pallet::getter(fn node_peer_id)]
	pub type AccountPeerMapping<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, (Ed25519PublicKey, Port, Ipv6Addr)>;

	/// Ed25519 public keys (aka peer ids) that are associated with account ids. (We keep track
	/// of them to ensure they don't somehow get reused between different account ids.)
	#[pallet::storage]
	#[pallet::getter(fn mapped_peer)]
	pub type MappedPeers<T: Config> = StorageMap<_, Blake2_128Concat, Ed25519PublicKey, ()>;

	/// Node CFE version.
	#[pallet::storage]
	#[pallet::getter(fn node_cfe_version)]
	pub type NodeCFEVersion<T: Config> =
		StorageMap<_, Blake2_128Concat, ValidatorIdOf<T>, Version, ValueQuery>;

	/// The last expired epoch index.
	#[pallet::storage]
	pub type LastExpiredEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// A map storing the expiry block numbers for old epochs.
	#[pallet::storage]
	pub type EpochExpiries<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, EpochIndex, OptionQuery>;

	/// A map between an epoch and the set of authorities (participating in this epoch).
	#[pallet::storage]
	pub type HistoricalAuthorities<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, Vec<ValidatorIdOf<T>>, ValueQuery>;

	/// A map between an epoch and the bonded balance (MAB)
	#[pallet::storage]
	pub type HistoricalBonds<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, T::Amount, ValueQuery>;

	/// A map between an authority and a set of all the active epochs a node was an authority in
	#[pallet::storage]
	pub type HistoricalActiveEpochs<T: Config> =
		StorageMap<_, Twox64Concat, ValidatorIdOf<T>, Vec<EpochIndex>, ValueQuery>;

	/// Backups, validator nodes who are not in the authority set.
	#[pallet::storage]
	#[pallet::getter(fn backups)]
	pub type Backups<T: Config> = StorageValue<_, BackupMap<T>, ValueQuery>;

	/// Determines the number of backup nodes who receive rewards as a percentage
	/// of the authority count.
	#[pallet::storage]
	#[pallet::getter(fn backup_reward_node_percentage)]
	pub type BackupRewardNodePercentage<T> = StorageValue<_, Percent, ValueQuery>;

	/// The absolute minimum number of authority nodes for the next epoch.
	#[pallet::storage]
	#[pallet::getter(fn authority_set_min_size)]
	pub type AuthoritySetMinSize<T> = StorageValue<_, AuthorityCount, ValueQuery>;

	/// Auction parameters.
	#[pallet::storage]
	#[pallet::getter(fn auction_parameters)]
	pub(super) type AuctionParameters<T: Config> = StorageValue<_, SetSizeParameters, ValueQuery>;

	/// An account's balance must be at least this percentage of the current bond in order to
	/// register as a validator.
	#[pallet::storage]
	#[pallet::getter(fn registration_mab_percentage)]
	pub(super) type RegistrationBondPercentage<T: Config> = StorageValue<_, Percent, ValueQuery>;

	/// Auction losers whose bids are below this percentage of the MAB will not be excluded from
	/// participating in Keygen.
	#[pallet::storage]
	#[pallet::getter(fn auction_bid_cutoff_percentage)]
	pub(super) type AuctionBidCutoffPercentage<T: Config> = StorageValue<_, Percent, ValueQuery>;

	/// Determines the minimum version that each CFE must report to be considered qualified
	/// for Keygen.
	#[pallet::storage]
	#[pallet::getter(fn minimum_reported_cfe_version)]
	pub(super) type MinimumReportedCfeVersion<T: Config> = StorageValue<_, SemVer, ValueQuery>;

	/// Determines the maximum allowed reduction of authority set size in percents between two
	/// consecutive epochs.
	#[pallet::storage]
	#[pallet::getter(fn max_authority_set_contraction_percentage)]
	pub(super) type MaxAuthoritySetContractionPercentage<T: Config> =
		StorageValue<_, Percent, ValueQuery>;

	/// Minimum bid amount required to participate in auctions.
	#[pallet::storage]
	pub type MinimumAuctionBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// Store the list of accounts that are active bidders.
	#[pallet::storage]
	#[pallet::getter(fn active_bidder)]
	pub type ActiveBidder<T: Config> = StorageValue<_, BTreeSet<T::AccountId>, ValueQuery>;

	/// Maps an operator account to it's exceptions. An exception is a delegator that is excluded
	/// from the operator's delegation acceptance configuration. If it's set to allow it means the
	/// delegator is not allowed to delegate if it's in the list of exceptions and vis versa for
	/// deny.
	#[pallet::storage]
	pub type Exceptions<T: Config> =
		StorageMap<_, Identity, T::AccountId, BTreeSet<T::AccountId>, ValueQuery>;

	/// Maps a managed validator to its operator.
	#[pallet::storage]
	pub type ManagedValidators<T: Config> =
		StorageMap<_, Identity, T::AccountId, T::AccountId, OptionQuery>;

	/// Maps a validator to the operators currently claiming it.
	#[pallet::storage]
	pub type ClaimedValidators<T: Config> =
		StorageMap<_, Identity, T::AccountId, BTreeSet<T::AccountId>, ValueQuery>;

	/// Maps an operator account to its configured settings.
	#[pallet::storage]
	pub type OperatorSettingsLookup<T: Config> =
		StorageMap<_, Identity, T::AccountId, OperatorSettings, OptionQuery>;

	/// Maps an delegator to an associated operator account.
	#[pallet::storage]
	pub type DelegationChoice<T: Config> =
		StorageMap<_, Identity, T::AccountId, T::AccountId, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The rotation is aborted
		RotationAborted,
		/// A new epoch has started \[epoch_index\]
		NewEpoch(EpochIndex),
		/// Rotation phase updated.
		RotationPhaseUpdated { new_phase: RotationPhase<T> },
		/// The CFE version has been updated.
		CFEVersionUpdated {
			account_id: ValidatorIdOf<T>,
			old_version: Version,
			new_version: Version,
		},
		/// An auction has a set of winners \[winners, bond\]
		AuctionCompleted(Vec<ValidatorIdOf<T>>, T::Amount),
		/// Some pallet configuration has been updated.
		PalletConfigUpdated { update: PalletConfigUpdate },
		/// An account has stopped bidding and will no longer take part in auctions.
		StoppedBidding { account_id: T::AccountId },
		/// A previously non-bidding account has started bidding.
		StartedBidding { account_id: T::AccountId },
		/// The rotation transaction(s) for the previous rotation are still pending to be
		/// succesfully broadcast, therefore, cannot start a new epoch rotation.
		PreviousRotationStillPending,
		/// A delegator has been blocked from delegating to an operator.
		DelegatorBlocked { delegator: T::AccountId, operator: T::AccountId },
		/// A delegator has been allowed to delegate to an operator.
		DelegatorAllowed { delegator: T::AccountId, operator: T::AccountId },
		/// A validator has been claimed by an operator.
		ValidatorClaimed { validator: T::AccountId, operator: T::AccountId },
		/// A validator has accepted the claim of an operator.
		OperatorAcceptedByValidator { validator: T::AccountId, operator: T::AccountId },
		/// A validator has been removed from an operator's managed pool.
		ValidatorRemovedFromOperator { validator: T::AccountId, operator: T::AccountId },
		/// Operator settings have been updated.
		OperatorSettingsUpdated { operator: T::AccountId, preferences: OperatorSettings },
		/// An account has undelegated from an operator.
		UnDelegated { delegator: T::AccountId, operator: T::AccountId },
		/// An account has delegated to an operator.
		Delegated { delegator: T::AccountId, operator: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Epoch duration supplied is invalid.
		InvalidEpochDuration,
		/// A rotation is in progress.
		RotationInProgress,
		/// Validator Peer mapping overlaps with an existing mapping.
		AccountPeerMappingOverlap,
		/// Invalid signature.
		InvalidAccountPeerMappingSignature,
		/// Invalid redemption period.
		InvalidRedemptionPeriod,
		/// Invalid minimum authority set size.
		InvalidAuthoritySetMinSize,
		/// Auction parameters are invalid.
		InvalidAuctionParameters,
		/// The dynamic set size ranges are inconsistent.
		InconsistentRanges,
		/// Not enough bidders were available to resolve the auction.
		NotEnoughBidders,
		/// Not enough funds to register as a validator.
		NotEnoughFunds,
		/// Rotations are currently disabled through SafeMode.
		RotationsDisabled,
		/// Validators cannot deregister until they are no longer key holders.
		StillKeyHolder,
		/// Validators cannot deregister until they stop bidding in the auction.
		StillBidding,
		/// Start Bidding is disabled due to Safe Mode.
		StartBiddingDisabled,
		/// Stop Bidding is disabled due to Safe Mode.
		StopBiddingDisabled,
		/// Can't stop bidding an account if it's already not bidding.
		AlreadyNotBidding,
		/// Can only start bidding if not already bidding.
		AlreadyBidding,
		/// We are in the auction phase
		AuctionPhase,
		/// Validator is already associated with an operator.
		AlreadyManagedByOperator,
		/// Validator does not exist.
		ValidatorDoesNotExist,
		/// Not authorized to perform this action.
		NotAuthorized,
		/// Operator is still associated with validators.
		StillAssociatedWithValidators,
		/// The validator is not claimed by any operator.
		NotClaimedByOperator,
		/// The provided account id has not the role validator.
		NotValidator,
		/// Operator is still being delegated to.
		StillAssociatedWithDelegators,
		/// The account is not delegating.
		AccountIsNotDelegating,
		/// Delegation is not available to validators or operators.
		DelegationNotAllowed,
		/// Can only delegate to accounts that are registered as operators.
		NotOperator,
		/// A delegator is either blocked or not explicitly allowed by the operator.
		DelegatorBlocked,
		/// The provided Operator fee is too low.
		OperatorFeeTooLow,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
			log::trace!(target: "cf-validator", "on_initialize: {:?}",CurrentRotationPhase::<T>::get());
			let mut weight = Weight::zero();

			weight.saturating_accrue(Self::punish_missed_authorship_slots());

			// Progress the authority rotation if necessary.
			weight.saturating_accrue(match CurrentRotationPhase::<T>::get() {
				RotationPhase::Idle => {
					if block_number.saturating_sub(CurrentEpochStartedAt::<T>::get()) >=
						EpochDuration::<T>::get() {
						if T::RotationBroadcastsPending::rotation_broadcasts_pending() {
							Self::deposit_event(Event::PreviousRotationStillPending);
							T::ValidatorWeightInfo::rotation_phase_idle()
						}
						else {
							Self::start_authority_rotation()
						}
					} else {
						T::ValidatorWeightInfo::rotation_phase_idle()
					}
				},
				RotationPhase::KeygensInProgress(mut rotation_state) => {
					let num_primary_candidates = rotation_state.num_primary_candidates();
					match T::KeyRotator::status() {
						AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete) => {
							Self::try_start_key_handover(rotation_state, block_number);
						},
						AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders)) => {
							rotation_state.ban(offenders);
							Self::try_restart_keygen(rotation_state);
						},
						AsyncResult::Pending => {
							log::debug!(target: "cf-validator", "awaiting keygen completion");
						},
						async_result => {
							debug_assert!(
								false,
								"Ready(KeygenComplete), Ready(Failed), Pending possible. Got: {async_result:?}"
							);
							log::error!(target: "cf-validator", "Ready(KeygenComplete), Ready(Failed), Pending possible. Got: {async_result:?}");
							Self::abort_rotation();
						},
					};
					T::ValidatorWeightInfo::rotation_phase_keygen(num_primary_candidates)
				},
				RotationPhase::KeyHandoversInProgress(mut rotation_state) => {
					let num_primary_candidates = rotation_state.num_primary_candidates();
					match T::KeyRotator::status() {
						AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete) => {
							T::KeyRotator::activate_keys();
							Self::set_rotation_phase(RotationPhase::ActivatingKeys(rotation_state));
						},
						AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders)) => {
							// NOTE: we distinguish between candidates (nodes currently selected to become next authorities)
							// and non-candidates (current authorities *not* currently selected to become next authorities).
							// The outcome of this failure depends on whether any of the candidates caused it:
							let num_failed_candidates = offenders.intersection(&rotation_state.authority_candidates()).count();
							// TODO: Punish a bit more here? Some of these nodes are already an authority and have failed to participate in handover.
							// So given they're already not going to be in the set, excluding them from the set may not be enough punishment.
							rotation_state.ban(offenders);
							if num_failed_candidates > 0 {
								log::warn!(
									"{num_failed_candidates} authority candidate(s) failed to participate in key handover. Retrying from keygen.",
								);
								Self::try_restart_keygen(rotation_state);
							} else {
								log::warn!(
									"Key handover attempt failed. Retrying with a new participant set.",
								);
								Self::try_start_key_handover(rotation_state, block_number)
							};
						},
						AsyncResult::Pending => {
							log::debug!(target: "cf-validator", "awaiting key handover completion");
						},
						async_result => {
							debug_assert!(
								false,
								"Ready(KeyHandoverComplete), Pending possible. Got: {async_result:?}"
							);
							log::error!(target: "cf-validator", "Ready(KeyHandoverComplete), Pending possible. Got: {async_result:?}");
							Self::abort_rotation();
						},
					}
					// TODO: Use correct weight
					T::ValidatorWeightInfo::rotation_phase_keygen(num_primary_candidates)
				}
				RotationPhase::ActivatingKeys(rotation_state) => {
					let num_primary_candidates = rotation_state.num_primary_candidates();
					match T::KeyRotator::status() {
						AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete) => {
							Self::set_rotation_phase(RotationPhase::NewKeysActivated(
								rotation_state,
							));
						},
						AsyncResult::Pending => {
							log::debug!(target: "cf-validator", "awaiting vault rotations");
						},
						async_result => {
							debug_assert!(
								false,
								"Pending, or Ready(RotationComplete) possible. Got: {async_result:?}"
							);
							log::error!(target: "cf-validator", "Pending and Ready(RotationComplete) possible. Got {async_result:?}");
							Self::abort_rotation();
						},
					}
					T::ValidatorWeightInfo::rotation_phase_activating_keys(num_primary_candidates)
				},
				// The new session will kick off the new epoch
				_ => Weight::from_parts(0, 0),
			});
			weight
		}

		fn on_idle(block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// Check expiry of epoch and store last expired.
			if let Some(epoch_to_expire) = EpochExpiries::<T>::take(block_number) {
				Self::expire_epochs_up_to(epoch_to_expire, remaining_weight)
			} else {
				Default::default()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// [GOVERNANCE] Update a pallet config item.
		///
		/// The dispatch origin of this function must be governance.
		#[pallet::call_index(0)]
		#[pallet::weight(T::ValidatorWeightInfo::update_pallet_config())]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			update: PalletConfigUpdate,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			match update {
				PalletConfigUpdate::AuctionBidCutoffPercentage { percentage } => {
					<AuctionBidCutoffPercentage<T>>::put(percentage);
				},
				PalletConfigUpdate::RedemptionPeriodAsPercentage { percentage } => {
					<RedemptionPeriodAsPercentage<T>>::put(percentage);
				},
				PalletConfigUpdate::RegistrationBondPercentage { percentage } => {
					<RegistrationBondPercentage<T>>::put(percentage);
				},
				PalletConfigUpdate::AuthoritySetMinSize { min_size } => {
					ensure!(
						min_size <= <Self as EpochInfo>::current_authority_count(),
						Error::<T>::InvalidAuthoritySetMinSize
					);

					AuthoritySetMinSize::<T>::put(min_size);
				},
				PalletConfigUpdate::BackupRewardNodePercentage { percentage } => {
					<BackupRewardNodePercentage<T>>::put(percentage);
				},
				PalletConfigUpdate::EpochDuration { blocks } => {
					ensure!(blocks > 0, Error::<T>::InvalidEpochDuration);
					EpochDuration::<T>::set(blocks.into());
				},
				PalletConfigUpdate::AuctionParameters { parameters } => {
					Self::try_update_auction_parameters(parameters)?;
				},
				PalletConfigUpdate::MinimumReportedCfeVersion { version } => {
					MinimumReportedCfeVersion::<T>::put(version);
				},
				PalletConfigUpdate::MaxAuthoritySetContractionPercentage { percentage } => {
					MaxAuthoritySetContractionPercentage::<T>::put(percentage);
				},
				PalletConfigUpdate::MinimumAuctionBid { minimum_flip_bid } => {
					MinimumAuctionBid::<T>::set(
						FLIPPERINOS_PER_FLIP.saturating_mul(minimum_flip_bid.into()).into(),
					);
				},
			}

			Self::deposit_event(Event::PalletConfigUpdated { update });

			Ok(())
		}

		/// Force a new epoch. From the next block we will try to move to a new
		/// epoch and rotate our validators.
		///
		/// The dispatch origin of this function must be governance.
		///
		/// ## Weight
		///
		/// The weight is related to the number of bidders. Getting that number is quite expensive
		/// so we use 2 * authority_count as an approximation.
		#[pallet::call_index(1)]
		#[pallet::weight(T::ValidatorWeightInfo::start_authority_rotation(
			<Pallet<T> as EpochInfo>::current_authority_count().saturating_mul(2)
		))]
		pub fn force_rotation(origin: OriginFor<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				CurrentRotationPhase::<T>::get() == RotationPhase::Idle,
				Error::<T>::RotationInProgress
			);
			ensure!(T::SafeMode::get().authority_rotation_enabled, Error::<T>::RotationsDisabled,);
			Self::start_authority_rotation();

			Ok(())
		}

		/// Allow a node to set their keys for upcoming sessions
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::call_index(2)]
		#[pallet::weight((< T as pallet_session::Config >::WeightInfo::set_keys(), DispatchClass::Operational))]
		pub fn set_keys(origin: OriginFor<T>, keys: T::Keys, proof: Vec<u8>) -> DispatchResult {
			T::AccountRoleRegistry::ensure_validator(origin.clone())?;
			<pallet_session::Pallet<T>>::set_keys(origin, keys, proof)?;
			Ok(())
		}

		/// Allow a node to link their validator id to a peer id
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::call_index(3)]
		#[pallet::weight((T::ValidatorWeightInfo::register_peer_id(), DispatchClass::Operational))]
		pub fn register_peer_id(
			origin: OriginFor<T>,
			peer_id: Ed25519PublicKey,
			port: Port,
			ip_address: Ipv6Addr,
			signature: Ed25519Signature,
		) -> DispatchResult {
			// TODO Consider ensuring is non-private IP / valid IP

			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;

			// Note: this signature is necessary to prevent "rogue key" attacks (by ensuring
			// that `account_id` holds the corresponding secret key for `peer_id`)
			// Note: This signature verify doesn't need replay protection as you need the
			// account_id's private key to pass the above ensure_validator which has replay
			// protection. Note: Decode impl for peer_id's type doesn't detect invalid PublicKeys,
			// so we rely on the RuntimePublic::verify call below to do that (which internally uses
			// ed25519_dalek::PublicKey::from_bytes to do it).
			ensure!(
				RuntimePublic::verify(&peer_id, &account_id.encode(), &signature),
				Error::<T>::InvalidAccountPeerMappingSignature
			);

			if let Some((existing_peer_id, existing_port, existing_ip_address)) =
				AccountPeerMapping::<T>::get(&account_id)
			{
				if (existing_peer_id, existing_port, existing_ip_address) ==
					(peer_id, port, ip_address)
				{
					// Mapping hasn't changed
					return Ok(())
				}

				if existing_peer_id != peer_id {
					ensure!(
						!MappedPeers::<T>::contains_key(peer_id),
						Error::<T>::AccountPeerMappingOverlap
					);
					MappedPeers::<T>::remove(existing_peer_id);
					MappedPeers::<T>::insert(peer_id, ());
				}
			} else {
				ensure!(
					!MappedPeers::<T>::contains_key(peer_id),
					Error::<T>::AccountPeerMappingOverlap
				);
				MappedPeers::<T>::insert(peer_id, ());
			}

			AccountPeerMapping::<T>::insert(&account_id, (peer_id, port, ip_address));

			let validator_id = <ValidatorIdOf<T> as IsType<
				<T as frame_system::Config>::AccountId,
			>>::from_ref(&account_id);

			T::CfePeerRegistration::peer_registered(
				validator_id.clone(),
				peer_id,
				port,
				ip_address,
			);

			Ok(())
		}

		/// Allow a validator to report their current cfe version. Update storage and emit event if
		/// version is different from storage.
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::call_index(4)]
		#[pallet::weight((T::ValidatorWeightInfo::cfe_version(), DispatchClass::Operational))]
		pub fn cfe_version(origin: OriginFor<T>, new_version: Version) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;
			let validator_id = <ValidatorIdOf<T> as IsType<
				<T as frame_system::Config>::AccountId,
			>>::from_ref(&account_id);
			NodeCFEVersion::<T>::try_mutate(validator_id, |current_version| {
				if *current_version != new_version {
					Self::deposit_event(Event::CFEVersionUpdated {
						account_id: validator_id.clone(),
						old_version: *current_version,
						new_version,
					});
					*current_version = new_version;
				}
				Ok(())
			})
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::ValidatorWeightInfo::register_as_validator())]
		pub fn register_as_validator(origin: OriginFor<T>) -> DispatchResult {
			let account_id: T::AccountId = ensure_signed(origin)?;
			if Self::current_authority_count() >= AuctionParameters::<T>::get().max_size {
				ensure!(
					T::FundingInfo::total_balance_of(&account_id) >=
						RegistrationBondPercentage::<T>::get() * Self::bond(),
					Error::<T>::NotEnoughFunds
				);
			}
			T::AccountRoleRegistry::register_as_validator(&account_id)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::ValidatorWeightInfo::deregister_as_validator())]
		pub fn deregister_as_validator(origin: OriginFor<T>) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_validator(origin.clone())?;
			ensure!(!Self::is_bidding(&account_id), Error::<T>::StillBidding);

			let validator_id = <ValidatorIdOf<T> as IsType<
				<T as frame_system::Config>::AccountId,
			>>::from_ref(&account_id);

			ensure!(!EpochHistory::<T>::is_keyholder(validator_id), Error::<T>::StillKeyHolder);

			// This can only error if the validator didn't register any keys, in which case we want
			// to continue with the deregistration anyway.
			let _ = pallet_session::Pallet::<T>::purge_keys(origin);

			if let Some((peer_id, _, _)) = AccountPeerMapping::<T>::take(&account_id) {
				MappedPeers::<T>::remove(peer_id);
				T::CfePeerRegistration::peer_deregistered(validator_id.clone(), peer_id);
			}

			T::AccountRoleRegistry::deregister_as_validator(&account_id)?;

			Ok(())
		}

		/// Signals a non-bidding node's intent to start bidding, and participate in the
		/// next auction. Should only be called if the account is in a non-bidding state.
		#[pallet::call_index(8)]
		#[pallet::weight(T::ValidatorWeightInfo::start_bidding())]
		pub fn start_bidding(origin: OriginFor<T>) -> DispatchResult {
			ensure!(T::SafeMode::get().start_bidding_enabled, Error::<T>::StartBiddingDisabled);
			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;
			Self::activate_bidding(&account_id)?;
			Self::deposit_event(Event::StartedBidding { account_id });
			Ok(())
		}

		/// Signals a node's intent to withdraw their funds after the next auction and desist
		/// from future auctions. Should only be called by accounts that are not already not
		/// bidding.
		#[pallet::call_index(9)]
		#[pallet::weight(T::ValidatorWeightInfo::stop_bidding())]
		pub fn stop_bidding(origin: OriginFor<T>) -> DispatchResult {
			ensure!(T::SafeMode::get().stop_bidding_enabled, Error::<T>::StopBiddingDisabled);

			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;

			ensure!(!Self::is_auction_phase(), Error::<T>::AuctionPhase);

			ActiveBidder::<T>::try_mutate(|bidders| {
				bidders.remove(&account_id).then_some(()).ok_or(Error::<T>::AlreadyNotBidding)
			})?;
			Self::deposit_event(Event::StoppedBidding { account_id });
			Ok(())
		}

		/// Executed by a operator to claim a validator. By calling this, the operator
		/// signals his wish to manage the validator in his delegated staking pool. The validator
		/// has to actively accept this invitation by calling the `accept_operator` extrinsic.
		#[pallet::call_index(10)]
		#[pallet::weight(T::ValidatorWeightInfo::claim_validator())]
		pub fn claim_validator(origin: OriginFor<T>, validator_id: T::AccountId) -> DispatchResult {
			let operator = T::AccountRoleRegistry::ensure_operator(origin)?;
			ensure!(
				!ManagedValidators::<T>::contains_key(&validator_id),
				Error::<T>::AlreadyManagedByOperator
			);
			ensure!(
				T::AccountRoleRegistry::has_account_role(&validator_id, AccountRole::Validator),
				Error::<T>::NotValidator
			);
			ClaimedValidators::<T>::append(&validator_id, &operator);
			Self::deposit_event(Event::ValidatorClaimed { validator: validator_id, operator });
			Ok(())
		}

		/// Executed by a validator to accept an operator's invitation to manage it.
		#[pallet::call_index(11)]
		#[pallet::weight(T::ValidatorWeightInfo::accept_operator())]
		pub fn accept_operator(origin: OriginFor<T>, operator: T::AccountId) -> DispatchResult {
			let validator_id = T::AccountRoleRegistry::ensure_validator(origin)?;
			ensure!(
				!ManagedValidators::<T>::contains_key(&validator_id),
				Error::<T>::AlreadyManagedByOperator
			);

			ClaimedValidators::<T>::try_mutate(&validator_id, |claimed_by| {
				if claimed_by.remove(&operator) {
					Ok(())
				} else {
					Err(Error::<T>::NotClaimedByOperator)
				}
			})?;

			ManagedValidators::<T>::insert(&validator_id, &operator);

			Self::deposit_event(Event::OperatorAcceptedByValidator {
				validator: validator_id,
				operator,
			});

			Ok(())
		}

		/// Executed by an operator or a validator to remove the validator from the operator's
		/// delegation association.
		#[pallet::call_index(12)]
		#[pallet::weight(T::ValidatorWeightInfo::remove_validator())]
		pub fn remove_validator(origin: OriginFor<T>, validator: T::AccountId) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			let operator =
				ManagedValidators::<T>::get(&validator).ok_or(Error::<T>::ValidatorDoesNotExist)?;
			ensure!(account_id == operator || account_id == validator, Error::<T>::NotAuthorized);
			ManagedValidators::<T>::remove(&validator);

			Self::deposit_event(Event::ValidatorRemovedFromOperator { validator, operator });

			Ok(())
		}

		/// Executed by an operator to update its operator settings.
		#[pallet::call_index(13)]
		#[pallet::weight(T::ValidatorWeightInfo::update_operator_settings())]
		pub fn update_operator_settings(
			origin: OriginFor<T>,
			preferences: OperatorSettings,
		) -> DispatchResult {
			let operator = T::AccountRoleRegistry::ensure_operator(origin)?;

			ensure!(preferences.fee_bps >= MIN_OPERATOR_FEE, Error::<T>::OperatorFeeTooLow);

			if let Some(current_preferences) = OperatorSettingsLookup::<T>::get(&operator) {
				if current_preferences.delegation_acceptance != preferences.delegation_acceptance {
					Exceptions::<T>::remove(&operator);
				}
			}

			OperatorSettingsLookup::<T>::insert(&operator, preferences.clone());

			Self::deposit_event(Event::OperatorSettingsUpdated { operator, preferences });
			Ok(())
		}

		/// Block a delegator.
		///
		/// If the operator is set to allow, the delegator will be added to the
		/// exceptions list, meaning they are not allowed to delegate to the operator.
		/// If the operator is set to deny, the delegator will be removed from the
		/// exceptions list, meaning they are allowed to delegate to the operator.
		///
		/// Additionally, if the delegator was previously delegating to the operator,
		/// it will be undelegated.
		#[pallet::call_index(14)]
		#[pallet::weight(T::ValidatorWeightInfo::block_delegator())]
		pub fn block_delegator(origin: OriginFor<T>, delegator: T::AccountId) -> DispatchResult {
			let operator = T::AccountRoleRegistry::ensure_operator(origin)?;

			// If the delegator is currently delegating to this operator, we need to
			// undelegate them first.
			let _ =
				DelegationChoice::<T>::try_mutate_exists(&delegator, |maybe_assigned_operator| {
					if let Some(assigned_operator) = maybe_assigned_operator.take() {
						if assigned_operator == operator {
							Self::deposit_event(Event::UnDelegated {
								delegator: delegator.clone(),
								operator: operator.clone(),
							});
							Ok(())
						} else {
							Err(())
						}
					} else {
						Err(())
					}
				});

			match OperatorSettingsLookup::<T>::get(&operator)
				.unwrap_or_default()
				.delegation_acceptance
			{
				DelegationAcceptance::Deny => {
					// If the operator is set to deny, exceptions are the delegators that are
					// allowed.
					Exceptions::<T>::mutate(&operator, |allowed| {
						allowed.remove(&delegator);
					});
				},
				DelegationAcceptance::Allow => {
					// If the operator is set to allow, exceptions are the delegators that are
					// blocked.
					Exceptions::<T>::mutate(&operator, |blocked| {
						blocked.insert(delegator.clone());
					});
				},
			}

			Self::deposit_event(Event::DelegatorBlocked { operator, delegator });

			Ok(())
		}

		/// Allow a delegator.
		///
		/// If the operator is set to deny, the delegator will be added to the
		/// exceptions list, meaning they are allowed to delegate to the operator.
		/// If the operator is set to allow, the delegator will be removed from the
		/// exceptions list, meaning they are not allowed to delegate to the operator.
		#[pallet::call_index(15)]
		#[pallet::weight(T::ValidatorWeightInfo::allow_delegator())]
		pub fn allow_delegator(origin: OriginFor<T>, delegator: T::AccountId) -> DispatchResult {
			let operator = T::AccountRoleRegistry::ensure_operator(origin)?;

			match OperatorSettingsLookup::<T>::get(&operator)
				.unwrap_or_default()
				.delegation_acceptance
			{
				DelegationAcceptance::Deny => {
					// If the operator is set to deny, exceptions are the delegators that are
					// allowed.
					Exceptions::<T>::mutate(&operator, |allowed| {
						allowed.insert(delegator.clone());
					});
				},
				DelegationAcceptance::Allow => {
					// If the operator is set to allow, exceptions are the delegators that are
					// blocked.
					Exceptions::<T>::mutate(&operator, |blocked| {
						blocked.remove(&delegator);
					});
				},
			}

			Self::deposit_event(Event::DelegatorAllowed { operator, delegator });

			Ok(())
		}

		/// Executed by an account to register as an operator.
		#[pallet::call_index(16)]
		#[pallet::weight(T::ValidatorWeightInfo::register_as_operator())]
		pub fn register_as_operator(
			origin: OriginFor<T>,
			settings: OperatorSettings,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			ensure!(settings.fee_bps >= MIN_OPERATOR_FEE, Error::<T>::OperatorFeeTooLow);
			T::AccountRoleRegistry::register_as_operator(&account_id)?;
			OperatorSettingsLookup::<T>::insert(&account_id, settings);
			Ok(())
		}

		/// Executed by an operator to deregister as an operator.
		#[pallet::call_index(17)]
		#[pallet::weight(T::ValidatorWeightInfo::deregister_as_operator())]
		pub fn deregister_as_operator(origin: OriginFor<T>) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_operator(origin)?;

			ensure!(
				Self::get_all_associations_by_operator(
					&account_id,
					AssociationToOperator::Validator
				)
				.is_empty(),
				Error::<T>::StillAssociatedWithValidators
			);

			ensure!(
				Self::get_all_associations_by_operator(
					&account_id,
					AssociationToOperator::Delegator
				)
				.is_empty(),
				Error::<T>::StillAssociatedWithDelegators
			);

			T::AccountRoleRegistry::deregister_as_operator(&account_id)?;

			Exceptions::<T>::remove(&account_id);
			OperatorSettingsLookup::<T>::remove(&account_id);

			Ok(())
		}

		#[pallet::call_index(18)]
		#[pallet::weight(T::ValidatorWeightInfo::delegate())]
		pub fn delegate(origin: OriginFor<T>, operator: T::AccountId) -> DispatchResult {
			let delegator = ensure_signed(origin)?;

			ensure!(
				!T::AccountRoleRegistry::has_account_role(&delegator, AccountRole::Validator),
				Error::<T>::DelegationNotAllowed
			);
			ensure!(
				!T::AccountRoleRegistry::has_account_role(&delegator, AccountRole::Operator),
				Error::<T>::DelegationNotAllowed
			);

			ensure!(
				T::AccountRoleRegistry::has_account_role(&operator, AccountRole::Operator),
				Error::<T>::NotOperator
			);

			ensure!(
				match OperatorSettingsLookup::<T>::get(&operator)
					.expect(
						"operator is forced to set valid preferences during account registration"
					)
					.delegation_acceptance
				{
					DelegationAcceptance::Allow =>
						!Exceptions::<T>::get(&operator).contains(&delegator),
					DelegationAcceptance::Deny =>
						Exceptions::<T>::get(&operator).contains(&delegator),
				},
				Error::<T>::DelegatorBlocked
			);

			DelegationChoice::<T>::mutate(&delegator, |maybe_operator| {
				if let Some(previous_operator) = maybe_operator.replace(operator.clone()) {
					Self::deposit_event(Event::UnDelegated {
						delegator: delegator.clone(),
						operator: previous_operator,
					});
				}
			});

			Self::deposit_event(Event::Delegated { delegator, operator });

			Ok(())
		}

		#[pallet::call_index(19)]
		#[pallet::weight(T::ValidatorWeightInfo::undelegate())]
		pub fn undelegate(origin: OriginFor<T>) -> DispatchResult {
			let delegator = ensure_signed(origin)?;

			let operator = DelegationChoice::<T>::take(&delegator)
				.ok_or(Error::<T>::AccountIsNotDelegating)?;

			Self::deposit_event(Event::UnDelegated { delegator, operator });

			Ok(())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_authorities: BTreeSet<ValidatorIdOf<T>>,
		pub genesis_backups: BackupMap<T>,
		pub epoch_duration: BlockNumberFor<T>,
		pub bond: T::Amount,
		pub redemption_period_as_percentage: Percent,
		pub backup_reward_node_percentage: Percent,
		pub authority_set_min_size: AuthorityCount,
		pub auction_parameters: SetSizeParameters,
		pub auction_bid_cutoff_percentage: Percent,
		pub max_authority_set_contraction_percentage: Percent,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_authorities: Default::default(),
				genesis_backups: Default::default(),
				epoch_duration: Zero::zero(),
				bond: Default::default(),
				redemption_period_as_percentage: Zero::zero(),
				backup_reward_node_percentage: Zero::zero(),
				authority_set_min_size: Zero::zero(),
				auction_parameters: SetSizeParameters {
					min_size: 3,
					max_size: 15,
					max_expansion: 5,
				},
				auction_bid_cutoff_percentage: Zero::zero(),
				max_authority_set_contraction_percentage: DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			use cf_primitives::GENESIS_EPOCH;
			LastExpiredEpoch::<T>::set(Default::default());
			EpochDuration::<T>::set(self.epoch_duration);
			CurrentRotationPhase::<T>::set(RotationPhase::Idle);
			RedemptionPeriodAsPercentage::<T>::set(self.redemption_period_as_percentage);
			BackupRewardNodePercentage::<T>::set(self.backup_reward_node_percentage);
			AuthoritySetMinSize::<T>::set(self.authority_set_min_size);
			MaxAuthoritySetContractionPercentage::<T>::set(
				self.max_authority_set_contraction_percentage,
			);

			CurrentEpoch::<T>::set(GENESIS_EPOCH);

			Pallet::<T>::try_update_auction_parameters(self.auction_parameters)
				.expect("we should provide valid auction parameters at genesis");

			AuctionBidCutoffPercentage::<T>::set(self.auction_bid_cutoff_percentage);

			self.genesis_authorities.iter().for_each(|v| {
				Pallet::<T>::activate_bidding(ValidatorIdOf::<T>::into_ref(v))
					.expect("The account was just created so this can't fail.")
			});
			self.genesis_backups.keys().for_each(|v| {
				Pallet::<T>::activate_bidding(ValidatorIdOf::<T>::into_ref(v))
					.expect("The account was just created so this can't fail.")
			});

			Pallet::<T>::initialise_new_epoch(
				GENESIS_EPOCH,
				&self.genesis_authorities.iter().cloned().collect(),
				self.bond,
				self.genesis_backups.clone(),
			);
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn last_expired_epoch() -> EpochIndex {
		LastExpiredEpoch::<T>::get()
	}

	fn current_authorities() -> Vec<Self::ValidatorId> {
		CurrentAuthorities::<T>::get()
	}

	fn authorities_at_epoch(epoch: u32) -> Vec<Self::ValidatorId> {
		HistoricalAuthorities::<T>::get(epoch)
	}

	fn current_authority_count() -> AuthorityCount {
		CurrentAuthorities::<T>::decode_non_dedup_len().unwrap_or_default() as AuthorityCount
	}

	fn authority_index(
		epoch_index: EpochIndex,
		account: &Self::ValidatorId,
	) -> Option<AuthorityCount> {
		AuthorityIndex::<T>::get(epoch_index, account)
	}

	fn bond() -> Self::Amount {
		Bond::<T>::get()
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn authority_count_at_epoch(epoch_index: EpochIndex) -> Option<AuthorityCount> {
		HistoricalAuthorities::<T>::decode_non_dedup_len(epoch_index).map(|l| l as AuthorityCount)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: Vec<Self::ValidatorId>,
	) {
		for (i, authority) in new_authorities.iter().enumerate() {
			AuthorityIndex::<T>::insert(epoch_index, authority, i as AuthorityCount);
			HistoricalActiveEpochs::<T>::append(authority, epoch_index);
		}
		HistoricalAuthorities::<T>::insert(epoch_index, new_authorities);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_authorities(authorities: Vec<Self::ValidatorId>) {
		CurrentAuthorities::<T>::put(authorities);
	}
}

/// Indicates to the session module if the session should be rotated.
///
/// Note: We need to rotate the session pallet twice in order to rotate in the new set of
/// validators due to a limitation in the design of the session pallet. See the
/// substrate issue https://github.com/paritytech/substrate/issues/8650 for context.
///
/// Also see [SessionManager::new_session] impl below.
impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
	fn should_end_session(_now: BlockNumberFor<T>) -> bool {
		matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::NewKeysActivated(_) | RotationPhase::SessionRotating(..)
		)
	}
}

impl<T: Config> Pallet<T> {
	/// Makes the transition to the next epoch.
	///
	/// Among other things, updates the authority, historical and backup sets.
	///
	/// Also triggers [T::EpochTransitionHandler::on_new_epoch] which may call into other pallets.
	///
	/// Note this function is not benchmarked - it is only ever triggered via the session pallet,
	/// which at the time of writing uses `T::BlockWeights::get().max_block` ie. it implicitly fills
	/// the block.
	fn transition_to_next_epoch(
		new_authorities: Vec<ValidatorIdOf<T>>,
		bond: <T as Chainflip>::Amount,
	) {
		log::debug!(target: "cf-validator", "Starting new epoch");

		// Update epoch numbers.
		let (old_epoch, new_epoch) = CurrentEpoch::<T>::mutate(|epoch| {
			*epoch = epoch.saturating_add(One::one());
			(*epoch - 1, *epoch)
		});

		// Set the expiry block number for the old epoch.
		EpochExpiries::<T>::insert(
			frame_system::Pallet::<T>::current_block_number() + EpochDuration::<T>::get(),
			old_epoch,
		);

		Self::initialise_new_epoch(
			new_epoch,
			&new_authorities,
			bond,
			Self::get_active_bids()
				.into_iter()
				.filter_map(|Bid { bidder_id, amount }| {
					if !new_authorities.contains(&bidder_id) {
						Some((bidder_id, amount))
					} else {
						None
					}
				})
				.collect(),
		);

		Self::deposit_event(Event::NewEpoch(new_epoch));
		T::EpochTransitionHandler::on_new_epoch(new_epoch);
	}

	fn expire_epoch(epoch: EpochIndex) {
		for authority in EpochHistory::<T>::epoch_authorities(epoch).iter() {
			EpochHistory::<T>::deactivate_epoch(authority, epoch);
			if EpochHistory::<T>::number_of_active_epochs_for_authority(authority) == 0 {
				T::ReputationResetter::reset_reputation(authority);
			}
			T::Bonder::update_bond(authority, EpochHistory::<T>::active_bond(authority));
		}
		T::EpochTransitionHandler::on_expired_epoch(epoch);

		let validators = HistoricalAuthorities::<T>::take(epoch);
		for validator in validators {
			AuthorityIndex::<T>::remove(epoch, validator);
		}
		HistoricalBonds::<T>::remove(epoch);
	}

	fn expire_epochs_up_to(latest_epoch_to_expire: EpochIndex, remaining_weight: Weight) -> Weight {
		let mut weight_used = Weight::from_parts(0, 0);
		LastExpiredEpoch::<T>::mutate(|last_expired_epoch| {
			let first_unexpired_epoch = *last_expired_epoch + 1;
			for epoch in first_unexpired_epoch..=latest_epoch_to_expire {
				let required_weight = T::ValidatorWeightInfo::expire_epoch(
					HistoricalAuthorities::<T>::decode_len(epoch).unwrap_or_default() as u32,
				);
				if remaining_weight.all_gte(weight_used.saturating_add(required_weight)) {
					log::info!(target: "cf-validator", "🚮 Expiring epoch {}.", epoch);
					Self::expire_epoch(epoch);
					weight_used.saturating_accrue(required_weight);
					*last_expired_epoch = epoch;
				} else {
					log::info!(
						target: "cf-validator",
						"🚮 Postponing expiry of epoch {}. Required/Available weights: {}/{}.",
						epoch,
						required_weight.ref_time(),
						remaining_weight.ref_time(),
					);
				}
			}
		});
		weight_used
	}

	/// Does all state updates related to the *new* epoch. Is also called at genesis to initialise
	/// pallet state. Should not update any external state that is not managed by the validator
	/// pallet, ie. should not call `on_new_epoch`. Also does not need to concern itself with
	/// expiries etc. that relate to the state of previous epochs.
	fn initialise_new_epoch(
		new_epoch: EpochIndex,
		new_authorities: &Vec<ValidatorIdOf<T>>,
		new_bond: T::Amount,
		backup_map: BackupMap<T>,
	) {
		CurrentAuthorities::<T>::put(new_authorities);
		HistoricalAuthorities::<T>::insert(new_epoch, new_authorities);

		Bond::<T>::set(new_bond);

		HistoricalBonds::<T>::insert(new_epoch, new_bond);

		new_authorities.iter().enumerate().for_each(|(index, account_id)| {
			AuthorityIndex::<T>::insert(new_epoch, account_id, index as AuthorityCount);
			EpochHistory::<T>::activate_epoch(account_id, new_epoch);
			T::Bonder::update_bond(account_id, EpochHistory::<T>::active_bond(account_id));
		});

		CurrentEpochStartedAt::<T>::set(frame_system::Pallet::<T>::current_block_number());

		// We've got new authorities, which means the backups may have changed.
		Backups::<T>::put(backup_map);
	}

	fn set_rotation_phase(new_phase: RotationPhase<T>) {
		log::debug!(target: "cf-validator", "Advancing rotation phase to: {new_phase:?}");
		CurrentRotationPhase::<T>::put(new_phase.clone());
		Self::deposit_event(Event::RotationPhaseUpdated { new_phase });
	}

	fn abort_rotation() {
		log::warn!(
			target: "cf-validator",
			"Aborting rotation at phase: {:?}.", CurrentRotationPhase::<T>::get()
		);
		T::KeyRotator::reset_key_rotation();
		Self::set_rotation_phase(RotationPhase::Idle);
		Self::deposit_event(Event::<T>::RotationAborted);
	}

	fn start_authority_rotation() -> Weight {
		if !T::SafeMode::get().authority_rotation_enabled {
			log::warn!(
				target: "cf-validator",
				"Failed to start Authority Rotation: Disabled due to Runtime Safe Mode."
			);
			return T::ValidatorWeightInfo::start_authority_rotation_while_disabled_by_safe_mode()
		}
		if !matches!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle) {
			log::error!(
				target: "cf-validator",
				"Failed to start authority rotation: Authority rotation already in progress."
			);
			return T::ValidatorWeightInfo::start_authority_rotation_while_disabled_by_safe_mode()
		}
		log::info!(target: "cf-validator", "Starting rotation");

		match SetSizeMaximisingAuctionResolver::try_new(
			T::EpochInfo::current_authority_count(),
			AuctionParameters::<T>::get(),
		)
		.and_then(|resolver| {
			resolver.resolve_auction(
				Self::get_qualified_bidders::<T::KeygenQualification>(),
				AuctionBidCutoffPercentage::<T>::get(),
			)
		}) {
			Ok(auction_outcome) => {
				Self::deposit_event(Event::AuctionCompleted(
					auction_outcome.winners.clone(),
					auction_outcome.bond,
				));
				debug_assert!(!auction_outcome.winners.is_empty());
				debug_assert!({
					let bids = Self::get_active_bids()
						.into_iter()
						.map(|bid| (bid.bidder_id, bid.amount))
						.collect::<BTreeMap<_, _>>();
					auction_outcome.winners.iter().map(|id| bids.get(id)).is_sorted_by_key(Reverse)
				});
				log::info!(
					target: "cf-validator",
					"Auction resolved with {} winners and {} losers. Bond will be {}FLIP.",
					auction_outcome.winners.len(),
					auction_outcome.losers.len(),
					UniqueSaturatedInto::<u128>::unique_saturated_into(auction_outcome.bond) /
					FLIPPERINOS_PER_FLIP,
				);

				// Without reading the full list of bidders we can't know the real number.
				// Use the winners and losers as an approximation.
				let weight = T::ValidatorWeightInfo::start_authority_rotation(
					(auction_outcome.winners.len() + auction_outcome.losers.len()) as u32,
				);

				Self::try_start_keygen(RotationState::from_auction_outcome::<T>(auction_outcome));

				weight
			},
			Err(e) => {
				log::warn!(target: "cf-validator", "auction failed due to error: {:?}", e);
				Self::abort_rotation();

				// Use an approximation again - see comment above.
				T::ValidatorWeightInfo::start_authority_rotation({
					Self::current_authority_count() + Self::backup_reward_nodes_limit() as u32
				})
			},
		}
	}

	fn try_restart_keygen(rotation_state: RuntimeRotationState<T>) {
		T::KeyRotator::reset_key_rotation();
		Self::try_start_keygen(rotation_state);
	}

	fn try_start_keygen(rotation_state: RuntimeRotationState<T>) {
		let candidates = rotation_state.authority_candidates();
		let SetSizeParameters { min_size, .. } = AuctionParameters::<T>::get();

		let min_size = sp_std::cmp::max(
			min_size,
			(Percent::one().saturating_sub(MaxAuthoritySetContractionPercentage::<T>::get())) *
				Self::current_authority_count(),
		);

		if (candidates.len() as u32) < min_size {
			log::warn!(
				target: "cf-validator",
				"Only {:?} authority candidates available, not enough to satisfy the minimum set size of {:?}. - aborting rotation.",
				candidates.len(),
				min_size
			);
			Self::abort_rotation();
		} else {
			T::KeyRotator::keygen(candidates, rotation_state.new_epoch_index);
			Self::set_rotation_phase(RotationPhase::KeygensInProgress(rotation_state));
			log::info!(target: "cf-validator", "Vault rotation initiated.");
		}
	}

	fn try_start_key_handover(
		rotation_state: RuntimeRotationState<T>,
		block_number: BlockNumberFor<T>,
	) {
		if !T::SafeMode::get().authority_rotation_enabled {
			log::warn!(
				target: "cf-validator",
				"Failed to start Key Handover: Disabled due to Runtime Safe Mode. Aborting Authority rotation."
			);
			Self::abort_rotation();
			return
		}

		let authority_candidates = rotation_state.authority_candidates();
		if let Some(sharing_participants) = helpers::select_sharing_participants(
			Self::current_consensus_success_threshold(),
			rotation_state.unbanned_current_authorities::<T>(),
			&authority_candidates,
			block_number.unique_saturated_into(),
		) {
			T::KeyRotator::key_handover(
				sharing_participants,
				authority_candidates,
				rotation_state.new_epoch_index,
			);
			Self::set_rotation_phase(RotationPhase::KeyHandoversInProgress(rotation_state));
		} else {
			log::warn!(
				target: "cf-validator",
				"Too many authorities have been banned from keygen. Key handover would fail. Aborting rotation."
			);
			Self::abort_rotation();
		}
	}

	/// Returns the number of backup nodes eligible for rewards
	pub fn backup_reward_nodes_limit() -> usize {
		BackupRewardNodePercentage::<T>::get() * Self::current_authority_count() as usize
	}

	/// Returns the bids of the highest funded backup nodes, who are eligible for the backup rewards
	/// sorted by bids highest to lowest.
	pub fn highest_funded_qualified_backup_node_bids(
	) -> impl Iterator<Item = Bid<ValidatorIdOf<T>, <T as Chainflip>::Amount>> {
		let mut backups = T::KeygenQualification::filter_qualified_by_key(
			Backups::<T>::get().into_iter().collect(),
			|(bidder_id, _bid)| bidder_id,
		);

		let limit = Self::backup_reward_nodes_limit();
		if limit < backups.len() {
			backups.select_nth_unstable_by_key(limit, |(_, amount)| Reverse(*amount));
			backups.truncate(limit);
		}

		backups.into_iter().map(|(bidder_id, amount)| Bid { bidder_id, amount })
	}

	/// Returns ids as BTreeSet for fast lookups
	pub fn highest_funded_qualified_backup_nodes_lookup() -> BTreeSet<ValidatorIdOf<T>> {
		Self::highest_funded_qualified_backup_node_bids()
			.map(|Bid { bidder_id, .. }| bidder_id)
			.collect()
	}

	fn punish_missed_authorship_slots() -> Weight {
		let mut num_missed_slots = 0;
		let session_validators = <pallet_session::Pallet<T>>::validators();
		for slot in T::MissedAuthorshipSlots::missed_slots() {
			num_missed_slots += 1;
			// https://github.com/chainflip-io/substrate/blob/c172d0f683fab3792b90d876fd6ca27056af9fe9/frame/aura/src/lib.rs#L97
			let authority_index = slot % session_validators.len() as u64;
			if let Some(id) = session_validators.get(authority_index as usize) {
				T::OffenceReporter::report(PalletOffence::MissedAuthorshipSlot, id.clone());
			} else {
				log::error!(
					"Invalid slot index {:?} when processing missed authorship slots.",
					slot
				);
			}
		}

		T::ValidatorWeightInfo::missed_authorship_slots(num_missed_slots)
	}

	fn try_update_auction_parameters(new_parameters: SetSizeParameters) -> Result<(), Error<T>> {
		SetSizeMaximisingAuctionResolver::try_new(
			T::EpochInfo::current_authority_count(),
			new_parameters,
		)?;
		AuctionParameters::<T>::put(new_parameters);
		Ok(())
	}

	/// The smallest number of parties that can generate a signature.
	fn current_consensus_success_threshold() -> AuthorityCount {
		cf_utilities::success_threshold_from_share_count(Self::current_authority_count())
	}

	/// Sets the `active` flag associated with the account to true, signalling that the account
	/// wishes to participate in auctions, to become a network authority.
	///
	/// Returns an error if the account is already bidding.
	fn activate_bidding(account_id: &T::AccountId) -> Result<(), Error<T>> {
		ActiveBidder::<T>::try_mutate(|active_bidders| {
			active_bidders
				.insert(account_id.clone())
				.then_some(())
				.ok_or(Error::AlreadyBidding)
		})
	}

	pub fn get_active_bids() -> Vec<Bid<ValidatorIdOf<T>, T::Amount>> {
		ActiveBidder::<T>::get()
			.into_iter()
			.map(|bidder_id| Bid {
				bidder_id: <ValidatorIdOf<T> as IsType<T::AccountId>>::from_ref(&bidder_id).clone(),
				amount: T::FundingInfo::balance(&bidder_id),
			})
			.collect()
	}

	pub fn get_qualified_bidders<Q: QualifyNode<ValidatorIdOf<T>>>(
	) -> Vec<Bid<ValidatorIdOf<T>, T::Amount>> {
		Q::filter_qualified_by_key(Self::get_active_bids(), |Bid { ref bidder_id, .. }| bidder_id)
	}

	pub fn is_bidding(account_id: &T::AccountId) -> bool {
		ActiveBidder::<T>::get().contains(account_id)
	}

	pub fn is_auction_phase() -> bool {
		if CurrentRotationPhase::<T>::get() != RotationPhase::Idle {
			return true
		}

		// current_block > start + ((epoch * epoch%_can_redeem))
		CurrentEpochStartedAt::<T>::get()
			.saturating_add(RedemptionPeriodAsPercentage::<T>::get() * EpochDuration::<T>::get()) <=
			frame_system::Pallet::<T>::current_block_number()
	}

	pub fn get_all_associations_by_operator(
		operator: &T::AccountId,
		association: AssociationToOperator,
	) -> BTreeMap<T::AccountId, T::Amount> {
		match association {
			AssociationToOperator::Validator => ManagedValidators::<T>::iter(),
			AssociationToOperator::Delegator => DelegationChoice::<T>::iter(),
		}
		.filter_map(|(account_id, managing_operator)| {
			if managing_operator == *operator {
				let balance = T::FundingInfo::balance(&account_id);
				Some((account_id, balance))
			} else {
				None
			}
		})
		.collect()
	}
}

pub struct EpochHistory<T>(PhantomData<T>);

impl<T: Config> HistoricalEpoch for EpochHistory<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type EpochIndex = EpochIndex;
	type Amount = T::Amount;
	fn epoch_authorities(epoch: Self::EpochIndex) -> Vec<Self::ValidatorId> {
		HistoricalAuthorities::<T>::get(epoch)
	}

	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount {
		HistoricalBonds::<T>::get(epoch)
	}

	fn active_epochs_for_authority(authority: &Self::ValidatorId) -> Vec<Self::EpochIndex> {
		HistoricalActiveEpochs::<T>::get(authority)
	}

	fn number_of_active_epochs_for_authority(authority: &Self::ValidatorId) -> u32 {
		HistoricalActiveEpochs::<T>::decode_len(authority).unwrap_or_default() as u32
	}

	fn deactivate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::mutate(authority, |active_epochs| {
			active_epochs.retain(|&x| x != epoch);
		});
	}

	fn activate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::append(authority, epoch);
	}

	fn active_bond(authority: &Self::ValidatorId) -> Self::Amount {
		Self::active_epochs_for_authority(authority)
			.iter()
			.map(|epoch| Self::epoch_bond(*epoch))
			.max()
			.unwrap_or_else(|| Self::Amount::from(0_u32))
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<ValidatorIdOf<T>> for Pallet<T> {
	/// If we have a set of confirmed validators we roll them in over two blocks. See the comment
	/// on `ShouldEndSession` for further context.
	///
	/// The first rotation queues the new validators, the next rotation queues `None`, and
	/// activates the queued validators.
	fn new_session(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		match CurrentRotationPhase::<T>::get() {
			RotationPhase::NewKeysActivated(rotation_state) => {
				let mut next_authorities: Vec<ValidatorIdOf<T>> =
					rotation_state.authority_candidates().into_iter().collect();

				let hash = frame_system::Pallet::<T>::parent_hash();
				let mut buf: [u8; 8] = [0; 8];
				buf.copy_from_slice(&hash.as_ref()[..8]);
				let seed_from_hash: u64 = u64::from_be_bytes(buf);
				WyRand::new_seed(seed_from_hash).shuffle(&mut next_authorities);

				Self::set_rotation_phase(RotationPhase::SessionRotating(
					next_authorities.clone(),
					rotation_state.bond,
				));

				Some(next_authorities)
			},
			RotationPhase::SessionRotating(..) => {
				Self::set_rotation_phase(RotationPhase::Idle);
				None
			},
			_ => None,
		}
	}

	/// These Validators' keys must be registered as part of the session pallet genesis.
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		let genesis_authorities = Self::current_authorities();
		if !genesis_authorities.is_empty() {
			frame_support::print(
				"No genesis authorities found! Make sure the Validator pallet is initialised before the Session pallet."
			);
		};
		Some(genesis_authorities.into_iter().collect())
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}

	/// The session is starting
	fn start_session(_start_index: SessionIndex) {
		if let RotationPhase::SessionRotating(authorities, bond) = CurrentRotationPhase::<T>::get()
		{
			Pallet::<T>::transition_to_next_epoch(authorities, bond)
		}
	}
}

impl<T: Config> EstimateNextSessionRotation<BlockNumberFor<T>> for Pallet<T> {
	fn average_session_length() -> BlockNumberFor<T> {
		Self::epoch_duration()
	}

	fn estimate_current_session_progress(now: BlockNumberFor<T>) -> (Option<Permill>, Weight) {
		(
			Some(Permill::from_rational(
				now.saturating_sub(CurrentEpochStartedAt::<T>::get()),
				EpochDuration::<T>::get(),
			)),
			T::DbWeight::get().reads(2),
		)
	}

	fn estimate_next_session_rotation(
		_now: BlockNumberFor<T>,
	) -> (Option<BlockNumberFor<T>>, Weight) {
		(
			Some(CurrentEpochStartedAt::<T>::get() + EpochDuration::<T>::get()),
			T::DbWeight::get().reads(2),
		)
	}
}

pub struct PeerMapping<T>(PhantomData<T>);

impl<T: Config> QualifyNode<<T as Chainflip>::ValidatorId> for PeerMapping<T> {
	fn is_qualified(validator_id: &<T as Chainflip>::ValidatorId) -> bool {
		AccountPeerMapping::<T>::contains_key(validator_id.into_ref())
	}
}

pub struct NotDuringRotation<T: Config>(PhantomData<T>);

impl<T: Config> ExecutionCondition for NotDuringRotation<T> {
	fn is_satisfied() -> bool {
		CurrentRotationPhase::<T>::get() == RotationPhase::Idle
	}
}

pub struct UpdateBackupMapping<T>(PhantomData<T>);

impl<T: Config> OnAccountFunded for UpdateBackupMapping<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn on_account_funded(validator_id: &Self::ValidatorId, amount: Self::Amount) {
		if <Pallet<T> as EpochInfo>::current_authorities().contains(validator_id) {
			return
		}

		Backups::<T>::mutate(|backups| {
			if amount.is_zero() {
				if backups.remove(validator_id).is_none() {
					#[cfg(not(test))]
					log::warn!("Tried to remove non-existent ValidatorId {validator_id:?}..");
					#[cfg(test)]
					panic!("Tried to remove non-existent ValidatorId {validator_id:?}..");
				}
			} else {
				backups.insert(validator_id.clone(), amount);
			}
		});
	}
}

impl<T: Config> AuthoritiesCfeVersions for Pallet<T> {
	/// Returns the percentage of current authorities that are compatible with the provided version.
	fn percent_authorities_compatible_with_version(version: SemVer) -> Percent {
		let current_authorities = CurrentAuthorities::<T>::get();
		let authorities_count = current_authorities.len() as u32;

		Percent::from_rational(
			current_authorities
				.into_iter()
				.filter(|validator_id| {
					NodeCFEVersion::<T>::get(validator_id).compatibility_with_runtime(version) ==
						CfeCompatibility::Compatible
				})
				.count() as u32,
			authorities_count,
		)
	}
}

pub struct RemoveVanityNames<T>(PhantomData<T>);

impl<T: Config> OnKilledAccount<T::AccountId> for RemoveVanityNames<T> {
	fn on_killed_account(who: &T::AccountId) {
		ActiveBidder::<T>::mutate(|bidders| bidders.remove(who));
	}
}

pub struct QualifyByCfeVersion<T>(PhantomData<T>);

impl<T: Config> QualifyNode<<T as Chainflip>::ValidatorId> for QualifyByCfeVersion<T> {
	fn is_qualified(validator_id: &<T as Chainflip>::ValidatorId) -> bool {
		NodeCFEVersion::<T>::get(validator_id) >= MinimumReportedCfeVersion::<T>::get()
	}

	fn filter_qualified(
		validators: BTreeSet<<T as Chainflip>::ValidatorId>,
	) -> BTreeSet<<T as Chainflip>::ValidatorId> {
		let min_version = MinimumReportedCfeVersion::<T>::get();
		validators
			.into_iter()
			.filter(|id| NodeCFEVersion::<T>::get(id) >= min_version)
			.collect()
	}
}

pub struct QualifyByMinimumBid<T>(PhantomData<T>);

impl<T: Config> QualifyNode<<T as Chainflip>::ValidatorId> for QualifyByMinimumBid<T> {
	fn is_qualified(validator_id: &<T as Chainflip>::ValidatorId) -> bool {
		T::FundingInfo::balance(validator_id.into_ref()) >= MinimumAuctionBid::<T>::get()
	}

	fn filter_qualified(
		validators: BTreeSet<<T as Chainflip>::ValidatorId>,
	) -> BTreeSet<<T as Chainflip>::ValidatorId> {
		let min_bid = MinimumAuctionBid::<T>::get();
		validators
			.into_iter()
			.filter(|id| T::FundingInfo::balance(id.into_ref()) >= min_bid)
			.collect()
	}
}

impl<T: Config> RedemptionCheck for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;
	fn ensure_can_redeem(validator_id: &Self::ValidatorId) -> DispatchResult {
		if Self::is_auction_phase() {
			ensure!(
				!ActiveBidder::<T>::get()
					.contains(<ValidatorIdOf<T> as IsType<T::AccountId>>::into_ref(validator_id)),
				Error::<T>::StillBidding
			);
		}

		Ok(())
	}
}
