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
	AuctionResult, Auctioneer, AuthoritySetSizeRange, BackupNodes, BackupOrPassive, BidderProvider,
	Chainflip, ChainflipAccount, ChainflipAccountState, EmergencyRotation, EpochInfo,
	QualifyValidator, RemainingBid, StakeHandler,
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
		/// Minimum number of authorities required
		#[pallet::constant]
		type MinAuthorities: Get<u32>;
		/// Ratio of current authorities to backups
		#[pallet::constant]
		type AuthorityToBackupRatio: Get<u32>;
		/// Percentage of backup nodes in authority set in a emergency rotation
		#[pallet::constant]
		type PercentageOfBackupNodesInEmergency: Get<u32>;
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

	/// Size range for number of authorities we want in our authority set
	#[pallet::storage]
	#[pallet::getter(fn current_authority_set_size_range)]
	pub(super) type CurrentAuthoritySetSizeRange<T: Config> =
		StorageValue<_, AuthoritySetSizeRange, ValueQuery>;

	/// List of bidders that were not winners of the last auction, sorted from
	/// highest to lowest bid.
	#[pallet::storage]
	#[pallet::getter(fn remaining_bidders)]
	pub(super) type RemainingBidders<T: Config> =
		StorageValue<_, Vec<RemainingBid<T::ValidatorId, T::Amount>>, ValueQuery>;

	/// A size calculated for our backup node group
	#[pallet::storage]
	#[pallet::getter(fn backup_group_size)]
	pub(super) type BackupGroupSize<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The lowest backup node bid
	#[pallet::storage]
	#[pallet::getter(fn lowest_backup_node_bid)]
	pub(super) type LowestBackupNodeBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// The highest passive node bid
	#[pallet::storage]
	#[pallet::getter(fn highest_passive_node_bid)]
	pub(super) type HighestPassiveNodeBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction has a set of winners \[winners\]
		AuctionCompleted(Vec<T::ValidatorId>),
		/// The authority set size range has changed \[before, after\]
		AuthoritySetSizeRangeChanged(AuthoritySetSizeRange, AuthoritySetSizeRange),
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
		/// - [AuthoritySetSizeRangeChanged](Event::AuthoritySetSizeRangeChanged)
		///
		/// ## Errors
		///
		/// - [InvalidRange](Error::InvalidRange)
		#[pallet::weight(T::WeightInfo::set_current_authority_set_size_range())]
		pub fn set_current_authority_set_size_range(
			origin: OriginFor<T>,
			range: AuthoritySetSizeRange,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let old = Self::set_active_range(range)?;
			Self::deposit_event(Event::AuthoritySetSizeRangeChanged(old, range));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub authority_set_size_range: AuthoritySetSizeRange,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			use sp_runtime::traits::Zero;

			Self { authority_set_size_range: (Zero::zero(), Zero::zero()) }
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			Pallet::<T>::set_active_range(self.authority_set_size_range)
				.expect("we should be able to set the range of the active set");
		}
	}
}

impl<T: Config> Pallet<T> {
	fn set_active_range(range: AuthoritySetSizeRange) -> Result<AuthoritySetSizeRange, Error<T>> {
		let (low, high) = range;
		ensure!(high >= low && low >= T::MinAuthorities::get(), Error::<T>::InvalidRange);
		let old = CurrentAuthoritySetSizeRange::<T>::get();
		if old != range {
			CurrentAuthoritySetSizeRange::<T>::put(range);
		}
		Ok(old)
	}
}

impl<T: Config> Auctioneer for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type Error = Error<T>;

	// Resolve an auction.  Bids are taken and are qualified. In doing so a `AuctionResult` is
	// returned with the winners of the auction and the MAB.  Unsuccessful bids are grouped for
	// potential backup validator candidates.  If we are in an emergency rotation then the strategy
	// of grouping is modified to avoid a superminority of low collateralised nodes.
	fn resolve_auction() -> Result<AuctionResult<Self::ValidatorId, Self::Amount>, Error<T>> {
		let mut bids = T::BidderProvider::get_bidders();
		// Determine if this validator is qualified for bidding
		bids.retain(|(validator_id, _)| T::ValidatorQualification::is_qualified(validator_id));
		let excluded = T::KeygenExclusionSet::get();
		bids.retain(|(validator_id, _)| !excluded.contains(validator_id));
		let number_of_bidders = bids.len() as u32;
		let (min_number_of_authorities, max_number_of_authorities) =
			CurrentAuthoritySetSizeRange::<T>::get();
		// Final rule - Confirm we have our set size
		ensure!(number_of_bidders >= min_number_of_authorities, {
			log::error!(
				"[cf-auction] insufficient bidders to proceed. {} < {}",
				number_of_bidders,
				min_number_of_authorities
			);
			Error::<T>::NotEnoughBidders
		});

		bids.sort_unstable_by_key(|k| k.1);
		bids.reverse();

		let mut target_authority_set_size =
			min(max_number_of_authorities, number_of_bidders) as usize;
		let mut next_authority_set: Vec<_> =
			bids.iter().take(target_authority_set_size as usize).collect();

		if T::EmergencyRotation::emergency_rotation_in_progress() {
			// We are interested in only have `PercentageOfBackupNodesInEmergency`
			// of existing BVs in the validating set.  We ensure this by using the last
			// MAB to understand who were BVs and ensure we only maintain the required
			// amount under this level to avoid a superminority of low collateralised
			// nodes.
			if let Some(new_target_authority_set_size) =
				next_authority_set.iter().position(|(_, amount)| amount < &T::EpochInfo::bond())
			{
				let number_of_existing_backup_nodes = (target_authority_set_size -
					new_target_authority_set_size) as u32 *
					(T::AuthorityToBackupRatio::get() - 1) /
					T::AuthorityToBackupRatio::get();

				let number_of_backup_validators_to_be_included = (number_of_existing_backup_nodes
					as u32)
					.saturating_mul(T::PercentageOfBackupNodesInEmergency::get()) /
					100;

				target_authority_set_size = new_target_authority_set_size +
					number_of_backup_validators_to_be_included as usize;

				next_authority_set.truncate(target_authority_set_size);
			}
		}

		let winners: Vec<_> = next_authority_set
			.iter()
			.map(|(validator_id, _)| (*validator_id).clone())
			.collect();

		let backup_group_size = target_authority_set_size as u32 / T::AuthorityToBackupRatio::get();

		let remaining_bidders: Vec<_> =
			bids.iter().skip(target_authority_set_size as usize).collect();

		RemainingBidders::<T>::put(remaining_bidders);
		BackupGroupSize::<T>::put(backup_group_size);

		Self::deposit_event(Event::AuctionCompleted(winners.clone()));

		let minimum_active_bid = next_authority_set.last().map(|(_, bid)| *bid).unwrap_or_default();

		Ok(AuctionResult { winners, minimum_active_bid })
	}

	// Update the state for backup and passive, as this can change every block
	fn update_backup_and_passive_states() {
		let remaining_bidders = RemainingBidders::<T>::get();
		let backup_validators = Self::current_backup_nodes(&remaining_bidders);
		let passive_nodes = Self::current_passive_nodes(&remaining_bidders);
		let lowest_backup_validator_bid = Self::lowest_bid(&backup_validators);
		let highest_passive_node_bid = Self::highest_bid(&passive_nodes);

		// TODO: Look into removing these, we should only need to set this in one place
		LowestBackupNodeBid::<T>::put(lowest_backup_validator_bid);
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
	fn current_backup_nodes(
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
				Self::lowest_bid(&Self::current_backup_nodes(remaining_bidders));

			let highest_passive_node_bid =
				Self::highest_bid(&Self::current_passive_nodes(remaining_bidders));

			LowestBackupNodeBid::<T>::put(lowest_backup_validator_bid);
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
				if amount > LowestBackupNodeBid::<T>::get() =>
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
				if amount != LowestBackupNodeBid::<T>::get() =>
			{
				let remaining_bidders = &mut RemainingBidders::<T>::get();
				Pallet::<T>::update_stake_for_bidder(
					remaining_bidders,
					(validator_id.clone(), amount),
				);
				if amount < LowestBackupNodeBid::<T>::get() {
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

impl<T: Config> BackupNodes for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn backup_nodes() -> Vec<Self::ValidatorId> {
		RemainingBidders::<T>::get()
			.iter()
			.take(BackupGroupSize::<T>::get() as usize)
			.map(|(validator_id, _)| validator_id.clone())
			.collect()
	}
}
