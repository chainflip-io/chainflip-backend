#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(is_sorted)]

mod mock;
mod tests;

mod helpers;

pub mod weights;
pub use weights::WeightInfo;

mod auction_resolver;
mod benchmarking;
mod rotation_state;

pub use auction_resolver::*;
use cf_primitives::{
	AuthorityCount, Ed25519PublicKey, EpochIndex, Ipv6Addr, SemVer,
	DEFAULT_MAX_AUTHORITY_SET_CONTRACTION, FLIPPERINOS_PER_FLIP,
};
use cf_traits::{
	impl_pallet_safe_mode, offence_reporting::OffenceReporter, AsyncResult, AuthoritiesCfeVersions,
	Bid, BidderProvider, Bonding, CfePeerRegistration, Chainflip, EpochInfo,
	EpochTransitionHandler, ExecutionCondition, FundingInfo, HistoricalEpoch,
	MissedAuthorshipSlots, OnAccountFunded, QualifyNode, ReputationResetter, SetSafeMode,
	VaultRotator,
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
	RegistrationBondPercentage { percentage: Percent },
	AuctionBidCutoffPercentage { percentage: Percent },
	RedemptionPeriodAsPercentage { percentage: Percent },
	BackupRewardNodePercentage { percentage: Percent },
	EpochDuration { blocks: u32 },
	AuthoritySetMinSize { min_size: AuthorityCount },
	AuctionParameters { parameters: SetSizeParameters },
	MinimumReportedCfeVersion { version: SemVer },
	MaxAuthoritySetContractionPercentage { percentage: Percent },
}

type RuntimeRotationState<T> =
	RotationState<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

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
	SessionRotating(RuntimeRotationState<T>),
}

type ValidatorIdOf<T> = <T as Chainflip>::ValidatorId;
type VanityName = Vec<u8>;

type BackupMap<T> = BTreeMap<ValidatorIdOf<T>, <T as Chainflip>::Amount>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	MissedAuthorshipSlot,
}

pub const MAX_LENGTH_FOR_VANITY_NAME: usize = 64;

impl_pallet_safe_mode!(PalletSafeMode; authority_rotation_enabled);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{AccountRoleRegistry, VaultStatus};
	use frame_support::sp_runtime::app_crypto::RuntimePublic;
	use pallet_session::WeightInfo as SessionWeightInfo;

	#[pallet::pallet]
	#[pallet::without_storage_info]
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

		type VaultRotator: VaultRotator<ValidatorId = ValidatorIdOf<Self>>;

		/// For retrieving missed authorship slots.
		type MissedAuthorshipSlots: MissedAuthorshipSlots;

		/// Used to get the list of bidding nodes in order initialise the backup set post-rotation.
		type BidderProvider: BidderProvider<
			ValidatorId = <Self as Chainflip>::ValidatorId,
			Amount = Self::Amount,
		>;

		/// Criteria that need to be fulfilled to qualify as a validator node (authority or backup).
		type KeygenQualification: QualifyNode<<Self as Chainflip>::ValidatorId>;

		/// For reporting missed authorship slots.
		type OffenceReporter: OffenceReporter<
			ValidatorId = ValidatorIdOf<Self>,
			Offence = Self::Offence,
		>;

		/// Updates the bond of an authority.
		type Bonder: Bonding<ValidatorId = ValidatorIdOf<Self>, Amount = Self::Amount>;

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
	#[pallet::getter(fn blocks_per_epoch)]
	pub type BlocksPerEpoch<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

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
	pub type CurrentAuthorities<T: Config> =
		StorageValue<_, BTreeSet<ValidatorIdOf<T>>, ValueQuery>;

	/// Vanity names of the validators stored as a Map with the current validator IDs as key.
	#[pallet::storage]
	#[pallet::getter(fn vanity_names)]
	pub type VanityNames<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, VanityName>, ValueQuery>;

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
		StorageMap<_, Twox64Concat, EpochIndex, BTreeSet<ValidatorIdOf<T>>, ValueQuery>;

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
		/// An authority has register her current PeerId \[account_id, public_key, port,
		/// ip_address\]
		PeerIdRegistered(T::AccountId, Ed25519PublicKey, Port, Ipv6Addr),
		/// A authority has unregistered her current PeerId \[account_id, public_key\]
		PeerIdUnregistered(T::AccountId, Ed25519PublicKey),
		/// Vanity Name for a node has been set \[account_id, vanity_name\]
		VanityNameSet(T::AccountId, VanityName),
		/// An auction has a set of winners \[winners, bond\]
		AuctionCompleted(Vec<ValidatorIdOf<T>>, T::Amount),
		/// Some pallet configuration has been updated.
		PalletConfigUpdated { update: PalletConfigUpdate },
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
		/// Vanity name length exceeds the limit of 64 characters.
		NameTooLong,
		/// Invalid characters in the name.
		InvalidCharactersInName,
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
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
			log::trace!(target: "cf-validator", "on_initialize: {:?}",CurrentRotationPhase::<T>::get());
			let mut weight = Weight::zero();

			// Check expiry of epoch and store last expired.
			if let Some(epoch_index) = EpochExpiries::<T>::take(block_number) {
				weight.saturating_accrue(Self::expire_epoch(epoch_index));
			}

			weight.saturating_accrue(Self::punish_missed_authorship_slots());

			// Progress the authority rotation if necessary.
			weight.saturating_accrue(match CurrentRotationPhase::<T>::get() {
				RotationPhase::Idle => {
					if block_number.saturating_sub(CurrentEpochStartedAt::<T>::get()) >=
						BlocksPerEpoch::<T>::get()
					{
						Self::start_authority_rotation()
					} else {
						T::ValidatorWeightInfo::rotation_phase_idle()
					}
				},
				RotationPhase::KeygensInProgress(mut rotation_state) => {
					let num_primary_candidates = rotation_state.num_primary_candidates();
					match T::VaultRotator::status() {
						AsyncResult::Ready(VaultStatus::KeygenComplete) => {
							Self::try_start_key_handover(rotation_state, block_number);
						},
						AsyncResult::Ready(VaultStatus::Failed(offenders)) => {
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
					match T::VaultRotator::status() {
						AsyncResult::Ready(VaultStatus::KeyHandoverComplete) => {
							let new_authorities = rotation_state.authority_candidates();
							HistoricalAuthorities::<T>::insert(rotation_state.new_epoch_index, new_authorities);
							T::VaultRotator::activate();
							Self::set_rotation_phase(RotationPhase::ActivatingKeys(rotation_state));
						},
						AsyncResult::Ready(VaultStatus::Failed(offenders)) => {
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
					match T::VaultRotator::status() {
						AsyncResult::Ready(VaultStatus::RotationComplete) => {
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
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// [GOVERNANCE] Update a pallet config item.
		///
		/// The dispatch origin of this function must be governance.
		///
		/// ## Events
		///
		/// - [PalletConfigUpdate](Event::PalletConfigUpdate)
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
					BlocksPerEpoch::<T>::set(blocks.into());
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
			}

			Self::deposit_event(Event::PalletConfigUpdated { update });

			Ok(())
		}

		/// Force a new epoch. From the next block we will try to move to a new
		/// epoch and rotate our validators.
		///
		/// The dispatch origin of this function must be root.
		///
		/// ## Events
		///
		/// - [ForceRotationRequested](Event::ForceRotationRequested)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		/// - [RotationInProgress](Error::RotationInProgress)
		///
		/// ## Weight
		///
		/// The weight is related to the number of bidders. Getting that number is quite expensive
		/// so we use 2 * authority_count as an approximation.
		#[pallet::call_index(1)]
		#[pallet::weight(T::ValidatorWeightInfo::start_authority_rotation(
			<Pallet<T> as EpochInfo>::current_authority_count().saturating_mul(2)
		))]
		pub fn force_rotation(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				CurrentRotationPhase::<T>::get() == RotationPhase::Idle,
				Error::<T>::RotationInProgress
			);
			ensure!(T::SafeMode::get().authority_rotation_enabled, Error::<T>::RotationsDisabled,);
			Self::start_authority_rotation();

			Ok(().into())
		}

		/// Allow a node to set their keys for upcoming sessions
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		///
		/// ## Dependencies
		///
		/// - [Session Pallet](pallet_session::Config)
		#[pallet::call_index(2)]
		#[pallet::weight(< T as pallet_session::Config >::WeightInfo::set_keys())] // TODO: check if this is really valid
		pub fn set_keys(
			origin: OriginFor<T>,
			keys: T::Keys,
			proof: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			T::AccountRoleRegistry::ensure_validator(origin.clone())?;
			<pallet_session::Pallet<T>>::set_keys(origin, keys, proof)?;
			Ok(().into())
		}

		/// Allow a node to link their validator id to a peer id
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [PeerIdRegistered](Event::PeerIdRegistered)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [InvalidAccountPeerMappingSignature](Error::InvalidAccountPeerMappingSignature)
		/// - [AccountPeerMappingOverlap](Error::AccountPeerMappingOverlap)
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::call_index(3)]
		#[pallet::weight(T::ValidatorWeightInfo::register_peer_id())]
		pub fn register_peer_id(
			origin: OriginFor<T>,
			peer_id: Ed25519PublicKey,
			port: Port,
			ip_address: Ipv6Addr,
			signature: Ed25519Signature,
		) -> DispatchResultWithPostInfo {
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
					return Ok(().into())
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

			// TODO: Consider removing this
			Self::deposit_event(Event::PeerIdRegistered(account_id, peer_id, port, ip_address));
			Ok(().into())
		}

		/// Allow a validator to report their current cfe version. Update storage and emit event if
		/// version is different from storage.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [CFEVersionUpdated](Event::CFEVersionUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::call_index(4)]
		#[pallet::weight(T::ValidatorWeightInfo::cfe_version())]
		pub fn cfe_version(
			origin: OriginFor<T>,
			new_version: Version,
		) -> DispatchResultWithPostInfo {
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
				Ok(().into())
			})
		}

		/// Allow a node to set a "Vanity Name" for themselves. This is functionally
		/// useless but can be used to make the network a bit more friendly for
		/// observers. Names are required to be <= MAX_LENGTH_FOR_VANITY_NAME (64)
		/// UTF-8 bytes.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [VanityNameSet](Event::VanityNameSet)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [NameTooLong](Error::NameTooLong)
		/// - [InvalidCharactersInName](Error::InvalidCharactersInName)
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::call_index(5)]
		#[pallet::weight(T::ValidatorWeightInfo::set_vanity_name())]
		pub fn set_vanity_name(origin: OriginFor<T>, name: Vec<u8>) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ensure!(name.len() <= MAX_LENGTH_FOR_VANITY_NAME, Error::<T>::NameTooLong);
			ensure!(sp_std::str::from_utf8(&name).is_ok(), Error::<T>::InvalidCharactersInName);
			let mut vanity_names = VanityNames::<T>::get();
			vanity_names.insert(account_id.clone(), name.clone());
			VanityNames::<T>::put(vanity_names);
			Self::deposit_event(Event::VanityNameSet(account_id, name));
			Ok(().into())
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
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_authorities: BTreeSet<ValidatorIdOf<T>>,
		pub genesis_backups: BackupMap<T>,
		pub genesis_vanity_names: BTreeMap<T::AccountId, VanityName>,
		pub blocks_per_epoch: BlockNumberFor<T>,
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
				genesis_vanity_names: Default::default(),
				blocks_per_epoch: Zero::zero(),
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
			BlocksPerEpoch::<T>::set(self.blocks_per_epoch);
			CurrentRotationPhase::<T>::set(RotationPhase::Idle);
			RedemptionPeriodAsPercentage::<T>::set(self.redemption_period_as_percentage);
			BackupRewardNodePercentage::<T>::set(self.backup_reward_node_percentage);
			AuthoritySetMinSize::<T>::set(self.authority_set_min_size);
			VanityNames::<T>::put(&self.genesis_vanity_names);
			MaxAuthoritySetContractionPercentage::<T>::set(
				self.max_authority_set_contraction_percentage,
			);

			CurrentEpoch::<T>::set(GENESIS_EPOCH);

			Pallet::<T>::try_update_auction_parameters(self.auction_parameters)
				.expect("we should provide valid auction parameters at genesis");

			AuctionBidCutoffPercentage::<T>::set(self.auction_bid_cutoff_percentage);

			Pallet::<T>::initialise_new_epoch(
				GENESIS_EPOCH,
				&self.genesis_authorities,
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

	fn current_authorities() -> BTreeSet<Self::ValidatorId> {
		CurrentAuthorities::<T>::get()
	}

	fn current_authority_count() -> AuthorityCount {
		CurrentAuthorities::<T>::decode_len().unwrap_or_default() as AuthorityCount
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

	fn is_auction_phase() -> bool {
		if CurrentRotationPhase::<T>::get() != RotationPhase::Idle {
			return true
		}

		// start + ((epoch * percentage) / 100)
		CurrentEpochStartedAt::<T>::get()
			.saturating_add(RedemptionPeriodAsPercentage::<T>::get() * BlocksPerEpoch::<T>::get()) <=
			frame_system::Pallet::<T>::current_block_number()
	}

	fn authority_count_at_epoch(epoch_index: EpochIndex) -> Option<AuthorityCount> {
		HistoricalAuthorities::<T>::decode_len(epoch_index).map(|l| l as AuthorityCount)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: BTreeSet<Self::ValidatorId>,
	) {
		for (i, authority) in new_authorities.iter().enumerate() {
			AuthorityIndex::<T>::insert(epoch_index, authority, i as AuthorityCount);
			HistoricalActiveEpochs::<T>::append(authority, epoch_index);
		}
		HistoricalAuthorities::<T>::insert(epoch_index, new_authorities);
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
			RotationPhase::NewKeysActivated(_) | RotationPhase::SessionRotating(_)
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
	fn transition_to_next_epoch(rotation_state: RuntimeRotationState<T>) {
		log::debug!(target: "cf-validator", "Starting new epoch");

		// Update epoch numbers.
		let (old_epoch, new_epoch) = CurrentEpoch::<T>::mutate(|epoch| {
			*epoch = epoch.saturating_add(One::one());
			(*epoch - 1, *epoch)
		});

		// Set the expiry block number for the old epoch.
		EpochExpiries::<T>::insert(
			frame_system::Pallet::<T>::current_block_number() + BlocksPerEpoch::<T>::get(),
			old_epoch,
		);

		let new_authorities = rotation_state.authority_candidates();

		Self::initialise_new_epoch(
			new_epoch,
			&new_authorities,
			rotation_state.bond,
			T::BidderProvider::get_bidders()
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
	}

	fn expire_epoch(epoch: EpochIndex) -> Weight {
		LastExpiredEpoch::<T>::set(epoch);
		let mut num_expired_authorities = 0;
		for authority in EpochHistory::<T>::epoch_authorities(epoch).iter() {
			num_expired_authorities += 1;
			EpochHistory::<T>::deactivate_epoch(authority, epoch);
			if EpochHistory::<T>::number_of_active_epochs_for_authority(authority) == 0 {
				T::ReputationResetter::reset_reputation(authority);
			}
			T::Bonder::update_bond(authority, EpochHistory::<T>::active_bond(authority));
		}
		T::EpochTransitionHandler::on_expired_epoch(epoch);
		T::ValidatorWeightInfo::expire_epoch(num_expired_authorities)
	}

	/// Does all state updates related to the *new* epoch. Is also called at genesis to initialise
	/// pallet state. Should not update any external state that is not managed by the validator
	/// pallet, ie. should not call `on_new_epoch`. Also does not need to concern itself with
	/// expiries etc. that relate to the state of previous epochs.
	fn initialise_new_epoch(
		new_epoch: EpochIndex,
		new_authorities: &BTreeSet<ValidatorIdOf<T>>,
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
		T::VaultRotator::reset_vault_rotation();
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
				T::BidderProvider::get_qualified_bidders::<T::KeygenQualification>(),
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
					let bids = T::BidderProvider::get_bidders()
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
		T::VaultRotator::reset_vault_rotation();
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
			T::VaultRotator::keygen(candidates, rotation_state.new_epoch_index);
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
			T::VaultRotator::key_handover(
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
		let mut backups: Vec<_> = Backups::<T>::get()
			.into_iter()
			.filter(|(bidder_id, _)| T::KeygenQualification::is_qualified(bidder_id))
			.collect();

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
}

pub struct EpochHistory<T>(PhantomData<T>);

impl<T: Config> HistoricalEpoch for EpochHistory<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type EpochIndex = EpochIndex;
	type Amount = T::Amount;
	fn epoch_authorities(epoch: Self::EpochIndex) -> BTreeSet<Self::ValidatorId> {
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
				let next_authorities = rotation_state.authority_candidates();
				Self::set_rotation_phase(RotationPhase::SessionRotating(rotation_state));
				Some(next_authorities.into_iter().collect())
			},
			RotationPhase::SessionRotating(_) => {
				Self::set_rotation_phase(RotationPhase::Idle);
				None
			},
			_ => None,
		}
	}

	/// These Validators' keys must be registered as part of the session pallet genesis.
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		let genesis_authorities = Self::current_authorities();
		assert!(
			!genesis_authorities.is_empty(),
			"No genesis authorities found! Make sure the Validator pallet is initialised before the Session pallet."
		);
		Some(genesis_authorities.into_iter().collect())
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}

	/// The session is starting
	fn start_session(_start_index: SessionIndex) {
		if let RotationPhase::SessionRotating(rotation_state) = CurrentRotationPhase::<T>::get() {
			Pallet::<T>::transition_to_next_epoch(rotation_state)
		}
	}
}

impl<T: Config> EstimateNextSessionRotation<BlockNumberFor<T>> for Pallet<T> {
	fn average_session_length() -> BlockNumberFor<T> {
		Self::blocks_per_epoch()
	}

	fn estimate_current_session_progress(now: BlockNumberFor<T>) -> (Option<Permill>, Weight) {
		(
			Some(Permill::from_rational(
				now.saturating_sub(CurrentEpochStartedAt::<T>::get()),
				BlocksPerEpoch::<T>::get(),
			)),
			T::DbWeight::get().reads(2),
		)
	}

	fn estimate_next_session_rotation(
		_now: BlockNumberFor<T>,
	) -> (Option<BlockNumberFor<T>>, Weight) {
		(
			Some(CurrentEpochStartedAt::<T>::get() + BlocksPerEpoch::<T>::get()),
			T::DbWeight::get().reads(2),
		)
	}
}

pub struct DeletePeerMapping<T: Config>(PhantomData<T>);

/// Implementation of `OnKilledAccount` ensures that we reconcile any flip dust remaining in the
/// account by burning it.
impl<T: Config> OnKilledAccount<T::AccountId> for DeletePeerMapping<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		if let Some((peer_id, _, _)) = AccountPeerMapping::<T>::take(account_id) {
			MappedPeers::<T>::remove(peer_id);

			let validator_id = <ValidatorIdOf<T> as IsType<
				<T as frame_system::Config>::AccountId,
			>>::from_ref(account_id);

			T::CfePeerRegistration::peer_deregistered(validator_id.clone(), peer_id);

			// TODO: consider removing this
			Pallet::<T>::deposit_event(Event::PeerIdUnregistered(account_id.clone(), peer_id));
		}
	}
}

pub struct DeleteVanityName<T: Config>(PhantomData<T>);

impl<T: Config> OnKilledAccount<T::AccountId> for DeleteVanityName<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		let mut vanity_names = VanityNames::<T>::get();
		vanity_names.remove(account_id);
		VanityNames::<T>::put(vanity_names);
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
					NodeCFEVersion::<T>::get(validator_id).is_compatible_with(version)
				})
				.count() as u32,
			authorities_count,
		)
	}
}

pub struct QualifyByCfeVersion<T>(PhantomData<T>);

impl<T: Config> QualifyNode<<T as Chainflip>::ValidatorId> for QualifyByCfeVersion<T> {
	fn is_qualified(validator_id: &<T as Chainflip>::ValidatorId) -> bool {
		NodeCFEVersion::<T>::get(validator_id) >= MinimumReportedCfeVersion::<T>::get()
	}

	fn filter_unqualified(
		validators: BTreeSet<<T as Chainflip>::ValidatorId>,
	) -> BTreeSet<<T as Chainflip>::ValidatorId> {
		let min_version = MinimumReportedCfeVersion::<T>::get();
		validators
			.into_iter()
			.filter(|id| NodeCFEVersion::<T>::get(id) >= min_version)
			.collect()
	}
}
