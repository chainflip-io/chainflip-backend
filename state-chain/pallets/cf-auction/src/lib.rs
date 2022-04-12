#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod auction_resolver;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

pub use weights::WeightInfo;

pub use auction_resolver::*;
use cf_traits::{
	AuctionOutcome, Auctioneer, BackupOrPassive, BackupValidators, BidderProvider, Chainflip,
	ChainflipAccount, ChainflipAccountState, EmergencyRotation, EpochInfo, QualifyValidator,
	RemainingBid, StakeHandler,
};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::One;
use sp_std::{cmp::min, collections::btree_set::BTreeSet, prelude::*};

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

type ActiveValidatorRange = (u32, u32);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(PALLET_VERSION)]
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
		/// Key generation exclusion set
		type KeygenExclusionSet: Get<BTreeSet<Self::ValidatorId>>;
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
			migrations::PalletMigration::<T>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade()
		}
	}

	/// Size range for number of validators we want in our validating set
	#[pallet::storage]
	#[pallet::getter(fn active_validator_size_range)]
	pub(super) type ActiveValidatorSizeRange<T: Config> =
		StorageValue<_, ActiveValidatorRange, ValueQuery>;

	/// List of bidders that were not winners of the last auction, sorted from
	/// highest to lowest bid.
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

	/// Auction parameters.
	#[pallet::storage]
	#[pallet::getter(fn auction_parameters)]
	pub(super) type AuctionParameters<T: Config> =
		StorageValue<_, <ResolverV1<T> as AuctionResolver<T>>::AuctionParameters, ValueQuery>;

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
		/// Invalid range used for the active validator range.
		InvalidRange,
		/// Not enough bidders were available to resolve the auction.
		NotEnoughBidders,
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
			use sp_runtime::traits::Zero;

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

impl<T: Config> Auctioneer<T> for Pallet<T> {
	type Error = Error<T>;

	fn resolve_auction() -> Result<AuctionOutcome<T>, Error<T>> {
		let mut bids = T::BidderProvider::get_bidders();
		// Determine if this validator is qualified for bidding
		bids.retain(|(validator_id, _)| T::ValidatorQualification::is_qualified(validator_id));
		let excluded = T::KeygenExclusionSet::get();
		bids.retain(|(validator_id, _)| !excluded.contains(validator_id));

		let outcome = ResolverV1::resolve_auction(&ActiveValidatorSizeRange::<T>::get(), bids)?;

		// TODO Move this to validator pallet.
		BackupGroupSize::<T>::put(outcome.losers.len() as u32);
		RemainingBidders::<T>::put(&outcome.losers);

		Self::deposit_event(Event::AuctionCompleted(outcome.winners.clone()));

		Ok(outcome)
	}
}

// TODO Move this & associated storage to validator pallet.
impl<T: Config> Pallet<T> {
	// Update the state for backup and passive, as this can change every block
	fn update_backup_and_passive_states() {
		let remaining_bidders = RemainingBidders::<T>::get();
		let backup_validators = Self::current_backup_validators(&remaining_bidders);
		let passive_nodes = Self::current_passive_nodes(&remaining_bidders);
		let lowest_backup_validator_bid = Self::lowest_bid(&backup_validators);
		let highest_passive_node_bid = Self::highest_bid(&passive_nodes);

		// TODO: Look into removing these, we should only need to set this in one place
		LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
		HighestPassiveNodeBid::<T>::put(highest_passive_node_bid);

		for (validator_id, _amount) in backup_validators {
			T::ChainflipAccount::set_backup_or_passive(
				&validator_id.into(),
				BackupOrPassive::Backup,
			);
		}

		for (validator_id, _amount) in passive_nodes {
			T::ChainflipAccount::set_backup_or_passive(
				&validator_id.into(),
				BackupOrPassive::Passive,
			);
		}
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

			// reverse sort by amount (highest first)
			remaining_bidders.sort_unstable_by_key(|k| k.1);
			remaining_bidders.reverse();

			let lowest_backup_validator_bid =
				Self::lowest_bid(&Self::current_backup_validators(remaining_bidders));

			let highest_passive_node_bid =
				Self::highest_bid(&Self::current_passive_nodes(remaining_bidders));

			LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
			HighestPassiveNodeBid::<T>::set(highest_passive_node_bid);
			RemainingBidders::<T>::put(remaining_bidders);
		}
	}

	// There are only a certain number of backup validators allowed to be backup
	// so when we update particular states, we must also adjust the one on the boundary
	fn set_validator_state_and_adjust_at_boundary(
		validator_id: &T::ValidatorId,
		backup_or_passive: BackupOrPassive,
		remaining_bidders: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>,
	) {
		T::ChainflipAccount::set_backup_or_passive(
			&(validator_id.clone().into()),
			backup_or_passive,
		);

		let index_of_shifted = if backup_or_passive == BackupOrPassive::Passive {
			BackupGroupSize::<T>::get().saturating_sub(One::one())
		} else {
			BackupGroupSize::<T>::get()
		};

		if let Some((adjusted_validator_id, _)) = remaining_bidders.get(index_of_shifted as usize) {
			T::ChainflipAccount::set_backup_or_passive(
				&(adjusted_validator_id.clone().into()),
				if backup_or_passive == BackupOrPassive::Backup {
					BackupOrPassive::Passive
				} else {
					BackupOrPassive::Backup
				},
			);
		}
	}

	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, Error<T>> {
		let (low, high) = range;
		ensure!(high >= low && low >= T::MinValidators::get(), Error::<T>::InvalidRange);
		let old = ActiveValidatorSizeRange::<T>::get();
		if old != range {
			ActiveValidatorSizeRange::<T>::put(range);
		}
		Ok(old)
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
			ChainflipAccountState::BackupOrPassive(BackupOrPassive::Passive)
				if amount > LowestBackupValidatorBid::<T>::get() =>
			{
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				// Update bid for bidder and state
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
				Pallet::<T>::set_validator_state_and_adjust_at_boundary(
					validator_id,
					BackupOrPassive::Backup,
					remaining_bidders,
				);
			},
			ChainflipAccountState::BackupOrPassive(BackupOrPassive::Passive)
				if amount > HighestPassiveNodeBid::<T>::get() =>
			{
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
			},
			ChainflipAccountState::BackupOrPassive(BackupOrPassive::Backup)
				if amount != LowestBackupValidatorBid::<T>::get() =>
			{
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
				if amount < LowestBackupValidatorBid::<T>::get() {
					Pallet::<T>::set_validator_state_and_adjust_at_boundary(
						validator_id,
						BackupOrPassive::Backup,
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
