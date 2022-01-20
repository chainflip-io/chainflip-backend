#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use cf_traits::{
	ActiveValidatorRange, AuctionError, AuctionIndex, AuctionPhase, AuctionResult, Auctioneer,
	BackupValidators, BidderProvider, ChainflipAccount, ChainflipAccountState, EmergencyRotation,
	HasPeerMapping, IsOnline, QualifyValidator, RemainingBid, StakeHandler, VaultRotationHandler,
	VaultRotator,
};
use frame_support::{pallet_prelude::*, sp_std::mem, traits::ValidatorRegistration};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_std::{cmp::min, prelude::*};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{
		AuctionIndex, AuctionResult, ChainflipAccount, EmergencyRotation, HasPeerMapping,
		RemainingBid, VaultRotator,
	};
	use frame_support::traits::ValidatorRegistration;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// An amount for a bid
		type Amount: Member
			+ Parameter
			+ Default
			+ Eq
			+ Ord
			+ Copy
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize;
		/// An identity for a validator
		type ValidatorId: Member
			+ Parameter
			+ Ord
			+ MaybeSerializeDeserialize
			+ Into<<Self as frame_system::Config>::AccountId>;
		/// Providing bidders
		type BidderProvider: BidderProvider<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
		/// To confirm we have a session key registered for a validator
		type Registrar: ValidatorRegistration<Self::ValidatorId>;
		/// Benchmark stuff
		type WeightInfo: WeightInfo;
		/// The lifecycle of a vault rotation
		type Handler: VaultRotator<ValidatorId = Self::ValidatorId>;
		/// For looking up Chainflip Account data.
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;
		/// An online validator
		type Online: IsOnline<ValidatorId = Self::ValidatorId>;
		/// A validator register their peer id
		type PeerMapping: HasPeerMapping<ValidatorId = Self::ValidatorId>;
		/// Emergency Rotations
		type EmergencyRotation: EmergencyRotation;
		/// Minimum amount of validators
		#[pallet::constant]
		type MinValidators: Get<u32>;
		/// Ratio of backup validators
		#[pallet::constant]
		type ActiveToBackupValidatorRatio: Get<u32>;
		/// Percentage of backup validators in validating set in a emergency rotation
		#[pallet::constant]
		type PercentageOfBackupValidatorsInEmergency: Get<u32>;
	}

	/// Pallet implements \[Hooks\] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current phase of the auction
	#[pallet::storage]
	#[pallet::getter(fn current_phase)]
	pub(super) type CurrentPhase<T: Config> =
		StorageValue<_, AuctionPhase<T::ValidatorId, T::Amount>, ValueQuery>;

	/// Current phase of the auction
	#[pallet::storage]
	#[pallet::getter(fn last_auction_result)]
	pub(super) type LastAuctionResult<T: Config> =
		StorageValue<_, AuctionResult<T::ValidatorId, T::Amount>, OptionQuery>;

	/// Size range for number of validators we want in our validating set
	#[pallet::storage]
	#[pallet::getter(fn active_validator_size_range)]
	pub(super) type ActiveValidatorSizeRange<T: Config> =
		StorageValue<_, ActiveValidatorRange, ValueQuery>;

	/// The index of the auction we are in
	#[pallet::storage]
	#[pallet::getter(fn current_auction_index)]
	pub(super) type CurrentAuctionIndex<T: Config> = StorageValue<_, AuctionIndex, ValueQuery>;

	/// The remaining set of bidders after an auction
	#[pallet::storage]
	#[pallet::getter(fn remaining_bidders)]
	pub(super) type RemainingBidders<T: Config> =
		StorageValue<_, Vec<RemainingBid<T::ValidatorId, T::Amount>>, ValueQuery>;

	/// A size calculated for our backup validator group
	#[pallet::storage]
	#[pallet::getter(fn backup_group_size)]
	pub(super) type BackupGroupSize<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The lowest backup validator bid
	#[pallet::storage]
	#[pallet::getter(fn lowest_backup_validator_bid)]
	pub(super) type LowestBackupValidatorBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// The highest passive validator bid
	#[pallet::storage]
	#[pallet::getter(fn highest_passive_node_bid)]
	pub(super) type HighestPassiveNodeBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[auction_index\]
		AuctionStarted(AuctionIndex),
		/// An auction has a set of winners \[auction_index, winners\]
		AuctionCompleted(AuctionIndex, Vec<T::ValidatorId>),
		/// The auction has been confirmed off-chain \[auction_index\]
		AuctionConfirmed(AuctionIndex),
		/// Awaiting bidders for the auction
		AwaitingBidders,
		/// The active validator range upper limit has changed \[before, after\]
		ActiveValidatorRangeChanged(ActiveValidatorRange, ActiveValidatorRange),
		/// The auction was aborted \[auction_index\]
		AuctionAborted(AuctionIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid auction index used in confirmation
		InvalidAuction,
		/// Invalid range used for the active validator range
		InvalidRange,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the size of our auction range
		///
		/// The dispatch origin of this function must be root.
		///
		/// ## Events
		///
		/// - [ActiveValidatorRangeChanged](Event::ActiveValidatorRangeChanged)
		///
		/// ## Errors
		///
		/// - [InvalidRange](Error::InvalidRange)
		#[pallet::weight(T::WeightInfo::set_active_validator_range())]
		pub fn set_active_validator_range(
			origin: OriginFor<T>,
			range: ActiveValidatorRange,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			match Self::set_active_range(range) {
				Ok(old) => {
					Self::deposit_event(Event::ActiveValidatorRangeChanged(old, range));
					Ok(().into())
				},
				Err(_) => Err(Error::<T>::InvalidRange.into()),
			}
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub validator_size_range: ActiveValidatorRange,
		pub winners: Vec<T::ValidatorId>,
		pub minimum_active_bid: T::Amount,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				validator_size_range: (Zero::zero(), Zero::zero()),
				winners: vec![],
				minimum_active_bid: Zero::zero(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Pallet::<T>::set_active_range(self.validator_size_range).expect("valid range");

			for validator_id in &self.winners {
				T::ChainflipAccount::update_state(
					&(validator_id.clone().into()),
					ChainflipAccountState::Validator,
				);
			}

			BackupGroupSize::<T>::put(
				self.winners.len() as u32 / T::ActiveToBackupValidatorRatio::get(),
			);

			LastAuctionResult::<T>::put(AuctionResult {
				winners: self.winners.clone(),
				minimum_active_bid: self.minimum_active_bid,
			});
		}
	}
}

impl<T: Config> QualifyValidator for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		// Rule #1 - They are registered
		// Rule #2 - They have a registered peer id
		// Rule #3 - Confirm that the validators are 'online'
		T::Registrar::is_registered(validator_id) &&
			T::PeerMapping::has_peer_mapping(validator_id) &&
			T::Online::is_online(validator_id)
	}
}

impl<T: Config> Auctioneer for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type BidderProvider = T::BidderProvider;

	fn auction_index() -> AuctionIndex {
		CurrentAuctionIndex::<T>::get()
	}

	fn active_range() -> ActiveValidatorRange {
		ActiveValidatorSizeRange::<T>::get()
	}

	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, AuctionError> {
		let (low, high) = range;

		if low >= high || low < T::MinValidators::get() {
			return Err(AuctionError::InvalidRange)
		}

		let old = ActiveValidatorSizeRange::<T>::get();
		if old == range {
			return Err(AuctionError::InvalidRange)
		}

		ActiveValidatorSizeRange::<T>::put(range);
		Ok(old)
	}

	fn auction_result() -> Option<AuctionResult<Self::ValidatorId, Self::Amount>> {
		LastAuctionResult::<T>::get()
	}

	fn phase() -> AuctionPhase<Self::ValidatorId, Self::Amount> {
		CurrentPhase::<T>::get()
	}

	fn waiting_on_bids() -> bool {
		mem::discriminant(&Self::phase()) == mem::discriminant(&AuctionPhase::default())
	}

	fn process() -> Result<AuctionPhase<Self::ValidatorId, Self::Amount>, AuctionError> {
		match <CurrentPhase<T>>::get() {
			// Run some basic rules on what we consider as valid bidders
			// At the moment this includes checking that their bid is more than 0, which
			// shouldn't be possible and whether they have registered their session keys
			// to be able to actual join the validating set.  If we manage to pass these tests
			// we kill the last set of winners stored, set the bond to 0, store this set of
			// bidders and change our state ready for an 'Auction' to be ran
			AuctionPhase::WaitingForBids => {
				// A new auction has started, store and emit the event
				CurrentAuctionIndex::<T>::mutate(|idx| *idx += 1);
				Self::deposit_event(Event::AuctionStarted(<CurrentAuctionIndex<T>>::get()));
				let mut bids = T::BidderProvider::get_bidders();
				// Number one rule - If we have a bid at 0 then please leave
				bids.retain(|(_, amount)| !amount.is_zero());
				// Determine if this validator is qualified for bidding
				bids.retain(|(validator_id, _)| {
					<Pallet<T> as QualifyValidator>::is_qualified(validator_id)
				});
				let number_of_bidders = bids.len() as u32;
				let (min_number_of_validators, max_number_of_validators) = ActiveValidatorSizeRange::<T>::get();
				// Final rule - Confirm we have our set size
				if number_of_bidders < min_number_of_validators {
					log::error!(
						"[cf-auction] insufficient bidders to proceed. {} < {}",
						number_of_bidders,
						min_number_of_validators
					);
					return Err(AuctionError::MinValidatorSize)
				};

				// We sort by bid and cut the size of the set based on auction size range
				// If we have a valid set, within the size range, we store this set as the
				// 'winners' of this auction, change the state to 'Completed' and store the
				// minimum bid needed to be included in the set.
				bids.sort_unstable_by_key(|k| k.1);
				bids.reverse();

				let mut target_validator_group_size =
					min(max_number_of_validators, number_of_bidders) as usize;
				let mut next_validator_group: Vec<_> =
					bids.iter().take(target_validator_group_size as usize).collect();

				if T::EmergencyRotation::emergency_rotation_in_progress() {
					// We are interested in only have `PercentageOfBackupValidatorsInEmergency`
					// of existing BVs in the validating set.  We ensure this by using the last
					// MAB to understand who were BVs and ensure we only maintain the required
					// amount under this level to avoid a superminority of low collateralised
					// nodes.
					if let Some(AuctionResult { minimum_active_bid, .. }) =
						LastAuctionResult::<T>::get()
					{
						if let Some(new_target_validator_group_size) = next_validator_group
							.iter()
							.position(|(_, amount)| amount < &minimum_active_bid)
						{
							let number_of_existing_backup_validators =
								(target_validator_group_size - new_target_validator_group_size)
									as u32 * (T::ActiveToBackupValidatorRatio::get() - 1) /
									T::ActiveToBackupValidatorRatio::get();

							let number_of_backup_validators_to_be_included =
								(number_of_existing_backup_validators as u32).saturating_mul(
									T::PercentageOfBackupValidatorsInEmergency::get(),
								) / 100;

							target_validator_group_size = new_target_validator_group_size +
								number_of_backup_validators_to_be_included as usize;

							next_validator_group.truncate(target_validator_group_size);
						}
					}
				}

				let minimum_active_bid =
					next_validator_group.last().map(|(_, bid)| *bid).unwrap_or_default();

				let validating_set: Vec<_> = next_validator_group
					.iter()
					.map(|(validator_id, _)| (*validator_id).clone())
					.collect();

				let backup_group_size =
					target_validator_group_size as u32 / T::ActiveToBackupValidatorRatio::get();

				let remaining_bidders: Vec<_> =
					bids.iter().skip(target_validator_group_size as usize).collect();

				let phase = AuctionPhase::ValidatorsSelected(
					validating_set.clone(),
					minimum_active_bid,
				);

				RemainingBidders::<T>::put(remaining_bidders);
				BackupGroupSize::<T>::put(backup_group_size);
				CurrentPhase::<T>::put(phase.clone());

				Self::deposit_event(Event::AuctionCompleted(
					<CurrentAuctionIndex<T>>::get(),
					validating_set.clone(),
				));

				T::Handler::start_vault_rotation(validating_set)
					.map_err(|_| AuctionError::Abort)?;

				return Ok(phase)

			},
			// Things have gone well and we have a set of 'Winners', congratulations.
			// We are ready to call this an auction a day resetting the bidders in storage and
			// setting the state ready for a new set of 'Bidders'
			AuctionPhase::ValidatorsSelected(winners, minimum_active_bid) => {
				match T::Handler::finalize_rotation() {
					Ok(_) => {
						let update_status = |validators: Vec<T::ValidatorId>, state| {
							for validator_id in validators {
								T::ChainflipAccount::update_state(&validator_id.into(), state);
							}
						};

						let remaining_bidders = RemainingBidders::<T>::get();
						let backup_validators = Self::current_backup_validators(&remaining_bidders);
						let passive_nodes = Self::current_passive_nodes(&remaining_bidders);
						let lowest_backup_validator_bid = Self::lowest_bid(&backup_validators);
						let highest_passive_node_bid = Self::highest_bid(&passive_nodes);

						LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
						HighestPassiveNodeBid::<T>::put(highest_passive_node_bid);

						update_status(winners.clone(), ChainflipAccountState::Validator);

						update_status(
							backup_validators
								.iter()
								.map(|(validator_id, _)| validator_id.clone())
								.collect(),
							ChainflipAccountState::Backup,
						);

						update_status(
							passive_nodes
								.iter()
								.map(|(validator_id, _)| validator_id.clone())
								.collect(),
							ChainflipAccountState::Passive,
						);


						// Store the result
						LastAuctionResult::<T>::put(AuctionResult { winners, minimum_active_bid });
						CurrentPhase::<T>::put(AuctionPhase::default());

						Self::deposit_event(Event::AuctionConfirmed(
							CurrentAuctionIndex::<T>::get(),
						));

						Ok(AuctionPhase::default())
					},
					Err(_) => Err(AuctionError::NotConfirmed),
				}
			},
		}
		.map_err(|e| {
			// Abort the process on error if not waiting for confirmation
			if e != AuctionError::NotConfirmed {
				Self::abort();
			}
			e
		})
	}

	fn abort() {
		CurrentPhase::<T>::put(AuctionPhase::default());
		Self::deposit_event(Event::AuctionAborted(CurrentAuctionIndex::<T>::get()));
	}
}

pub struct VaultRotationEventHandler<T>(PhantomData<T>);

impl<T: Config> VaultRotationHandler for VaultRotationEventHandler<T> {
	type ValidatorId = T::ValidatorId;

	fn vault_rotation_aborted() {
		Pallet::<T>::abort();
	}
}

impl<T: Config> Pallet<T> {
	fn current_backup_validators(
		remaining_bidders: &[RemainingBid<T::ValidatorId, T::Amount>],
	) -> Vec<RemainingBid<T::ValidatorId, T::Amount>> {
		remaining_bidders
			.iter()
			.take(BackupGroupSize::<T>::get() as usize)
			.cloned()
			.collect()
	}

	fn current_passive_nodes(
		remaining_bidders: &[RemainingBid<T::ValidatorId, T::Amount>],
	) -> Vec<RemainingBid<T::ValidatorId, T::Amount>> {
		remaining_bidders
			.iter()
			.skip(BackupGroupSize::<T>::get() as usize)
			.cloned()
			.collect()
	}

	fn lowest_bid(remaining_bidders: &[RemainingBid<T::ValidatorId, T::Amount>]) -> T::Amount {
		remaining_bidders.last().map(|(_, amount)| *amount).unwrap_or_default()
	}

	fn highest_bid(remaining_bidders: &[RemainingBid<T::ValidatorId, T::Amount>]) -> T::Amount {
		remaining_bidders.first().map(|(_, amount)| *amount).unwrap_or_default()
	}

	fn update_stake_for_bidder(
		remaining_bidders: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>,
		new_bid: RemainingBid<T::ValidatorId, T::Amount>,
	) {
		if let Ok(index) = remaining_bidders.binary_search_by(|bid| new_bid.0.cmp(&bid.0)) {
			remaining_bidders[index] = new_bid;
			Pallet::<T>::sort_remaining_bidders(remaining_bidders);
		}
	}

	fn sort_remaining_bidders(remaining_bids: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>) {
		// Sort and set state
		remaining_bids.sort_unstable_by_key(|k| k.1);
		remaining_bids.reverse();

		let lowest_backup_validator_bid =
			Self::lowest_bid(&Self::current_backup_validators(remaining_bids));

		let highest_passive_node_bid =
			Self::highest_bid(&Self::current_passive_nodes(remaining_bids));

		LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
		HighestPassiveNodeBid::<T>::set(highest_passive_node_bid);
		RemainingBidders::<T>::put(remaining_bids);
	}

	fn promote_or_demote(promote: bool, validator_id: &T::ValidatorId) {
		T::ChainflipAccount::update_state(
			&(validator_id.clone().into()),
			if promote { ChainflipAccountState::Backup } else { ChainflipAccountState::Passive },
		);
	}

	fn adjust_group(
		validator_id: &T::ValidatorId,
		promote: bool,
		remaining_bidders: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>,
	) {
		Self::promote_or_demote(promote, validator_id);

		let index_of_shifted = if !promote {
			BackupGroupSize::<T>::get().saturating_sub(One::one())
		} else {
			BackupGroupSize::<T>::get()
		};

		if let Some((adjusted_validator_id, _)) = remaining_bidders.get(index_of_shifted as usize) {
			Self::promote_or_demote(!promote, adjusted_validator_id);
		}
	}
}

pub struct HandleStakes<T>(PhantomData<T>);
impl<T: Config> StakeHandler for HandleStakes<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn stake_updated(validator_id: &Self::ValidatorId, amount: Self::Amount) {
		// This would only happen if we had a active set of less than 3, not likely
		if BackupGroupSize::<T>::get() == 0 {
			return
		}

		// We validate that the staker is qualified and can be considered to be a BV if the stake
		// meets the requirements
		if !<Pallet<T> as QualifyValidator>::is_qualified(validator_id) {
			return
		}

		if Pallet::<T>::waiting_on_bids() {
			match T::ChainflipAccount::get(&(validator_id.clone().into())).state {
				ChainflipAccountState::Passive => {
					if amount > LowestBackupValidatorBid::<T>::get() {
						let remaining_bidders = &mut RemainingBidders::<T>::get();
						// Update bid for bidder and state
						Pallet::<T>::update_stake_for_bidder(
							remaining_bidders,
							(validator_id.clone(), amount),
						);
						Pallet::<T>::adjust_group(validator_id, true, remaining_bidders);
					} else if amount > HighestPassiveNodeBid::<T>::get() {
						let remaining_bidders = &mut RemainingBidders::<T>::get();
						Pallet::<T>::update_stake_for_bidder(
							remaining_bidders,
							(validator_id.clone(), amount),
						);
					}
				},
				ChainflipAccountState::Backup =>
					if amount != LowestBackupValidatorBid::<T>::get() {
						let remaining_bidders = &mut RemainingBidders::<T>::get();
						Pallet::<T>::update_stake_for_bidder(
							remaining_bidders,
							(validator_id.clone(), amount),
						);
						if amount < LowestBackupValidatorBid::<T>::get() {
							Pallet::<T>::adjust_group(
								validator_id,
								false,
								&mut RemainingBidders::<T>::get(),
							);
						}
					},
				_ => {},
			}
		}
	}
}

impl<T: Config> BackupValidators for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn backup_validators() -> Vec<Self::ValidatorId> {
		RemainingBidders::<T>::get()
			.iter()
			.take(BackupGroupSize::<T>::get() as usize)
			.map(|(validator_id, _)| validator_id.clone())
			.collect()
	}
}
