#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

pub use weights::WeightInfo;

use cf_traits::{
	ActiveValidatorRange, AuctionError, AuctionResult, Auctioneer, BackupValidators,
	BidderProvider, Chainflip, ChainflipAccount, ChainflipAccountState, EmergencyRotation,
	EpochInfo, QualifyValidator, RemainingBid, StakeHandler,
};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{One, Zero};
use sp_std::{cmp::min, prelude::*};

pub mod releases {
	use frame_support::traits::StorageVersion;
	// Genesis version
	pub const V0: StorageVersion = StorageVersion::new(0);
	// Version 1 - Remove AuctionPhase and LastAuctionResult
	pub const V1: StorageVersion = StorageVersion::new(1);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(releases::V1)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Providing bidders
		type BidderProvider: BidderProvider<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
		/// Benchmark stuff
		type WeightInfo: WeightInfo;
		/// For looking up Chainflip Account data.
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;
		/// Emergency Rotations
		type EmergencyRotation: EmergencyRotation;
		/// Qualify a validator
		type ValidatorQualification: QualifyValidator<ValidatorId = Self::ValidatorId>;
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
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				releases::V1.put::<Pallet<T>>();
				migrations::v1::migrate::<T>().saturating_add(T::DbWeight::get().reads_writes(1, 1))
			} else {
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::pre_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			if releases::V1 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::post_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}
	}

	/// Size range for number of validators we want in our validating set
	#[pallet::storage]
	#[pallet::getter(fn active_validator_size_range)]
	pub(super) type ActiveValidatorSizeRange<T: Config> =
		StorageValue<_, ActiveValidatorRange, ValueQuery>;

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
		/// An auction has a set of winners \[auction_index, winners\]
		AuctionCompleted(Vec<T::ValidatorId>),
		/// The active validator range upper limit has changed \[before, after\]
		ActiveValidatorRangeChanged(ActiveValidatorRange, ActiveValidatorRange),
	}

	#[pallet::error]
	pub enum Error<T> {
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
			let old = Self::set_active_range(range)?;
			Self::deposit_event(Event::ActiveValidatorRangeChanged(old, range));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub validator_size_range: ActiveValidatorRange,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self { validator_size_range: (Zero::zero(), Zero::zero()) }
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			Pallet::<T>::set_active_range(self.validator_size_range)
				.expect("we should be able to set the range of the active set");
		}
	}
}

impl<T: Config> Pallet<T> {
	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, Error<T>> {
		let (low, high) = range;

		if low >= high || low < T::MinValidators::get() {
			return Err(Error::<T>::InvalidRange)
		}

		let old = ActiveValidatorSizeRange::<T>::get();
		if old == range {
			return Err(Error::<T>::InvalidRange)
		}

		ActiveValidatorSizeRange::<T>::put(range);
		Ok(old)
	}
}

impl<T: Config> Auctioneer for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	// Resolve an auction.  Bids are taken and are qualified. In doing so a `AuctionResult` is
	// returned with the winners of the auction and the MAB.  Unsuccessful bids are grouped for
	// potential backup validator candidates.  If we are in an emergency rotation then the strategy
	// of grouping is modified to avoid a superminority of low collateralised nodes.
	fn resolve_auction() -> Result<AuctionResult<Self::ValidatorId, Self::Amount>, AuctionError> {
		let mut bids = T::BidderProvider::get_bidders();
		// Determine if this validator is qualified for bidding
		bids.retain(|(validator_id, _)| T::ValidatorQualification::is_qualified(validator_id));
		let number_of_bidders = bids.len() as u32;
		let (min_number_of_validators, max_number_of_validators) =
			ActiveValidatorSizeRange::<T>::get();
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
			if let Some(new_target_validator_group_size) = next_validator_group
				.iter()
				.position(|(_, amount)| amount < &T::EpochInfo::bond())
			{
				let number_of_existing_backup_validators = (target_validator_group_size -
					new_target_validator_group_size) as u32 *
					(T::ActiveToBackupValidatorRatio::get() - 1) /
					T::ActiveToBackupValidatorRatio::get();

				let number_of_backup_validators_to_be_included =
					(number_of_existing_backup_validators as u32)
						.saturating_mul(T::PercentageOfBackupValidatorsInEmergency::get()) /
						100;

				target_validator_group_size = new_target_validator_group_size +
					number_of_backup_validators_to_be_included as usize;

				next_validator_group.truncate(target_validator_group_size);
			}
		}

		let minimum_active_bid =
			next_validator_group.last().map(|(_, bid)| *bid).unwrap_or_default();

		let winners: Vec<_> = next_validator_group
			.iter()
			.map(|(validator_id, _)| (*validator_id).clone())
			.collect();

		let backup_group_size =
			target_validator_group_size as u32 / T::ActiveToBackupValidatorRatio::get();

		let remaining_bidders: Vec<_> =
			bids.iter().skip(target_validator_group_size as usize).collect();

		RemainingBidders::<T>::put(remaining_bidders);
		BackupGroupSize::<T>::put(backup_group_size);

		Self::deposit_event(Event::AuctionCompleted(winners.clone()));

		Ok(AuctionResult { winners, minimum_active_bid })
	}

	// Things have gone well and we have a set of 'Winners', congratulations.
	// We are ready to call this an auction a day resetting the bidders in storage and
	// setting the state ready for a new set of 'Bidders'
	fn update_validator_status(auction: AuctionResult<Self::ValidatorId, Self::Amount>) {
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

		update_status(auction.winners, ChainflipAccountState::Validator);

		update_status(
			backup_validators.iter().map(|(validator_id, _)| validator_id.clone()).collect(),
			ChainflipAccountState::Backup,
		);

		update_status(
			passive_nodes.iter().map(|(validator_id, _)| validator_id.clone()).collect(),
			ChainflipAccountState::Passive,
		);
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
		// We validate that the staker is qualified and can be considered to be a BV if the stake
		// meets the requirements
		if !T::ValidatorQualification::is_qualified(validator_id) {
			return
		}

		// This would only happen if we had a active set of less than 3, not likely
		if BackupGroupSize::<T>::get() == 0 {
			return
		}

		match T::ChainflipAccount::get(&(validator_id.clone().into())).state {
			ChainflipAccountState::Passive if amount > LowestBackupValidatorBid::<T>::get() => {
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				// Update bid for bidder and state
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
				Pallet::<T>::adjust_group(validator_id, true, remaining_bidders);
			},
			ChainflipAccountState::Passive if amount > HighestPassiveNodeBid::<T>::get() => {
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
			},
			ChainflipAccountState::Backup if amount != LowestBackupValidatorBid::<T>::get() => {
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
