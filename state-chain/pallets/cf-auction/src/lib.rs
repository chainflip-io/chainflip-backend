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
	AuctionOutcome, Auctioneer, BidderProvider, Chainflip, ChainflipAccount, EmergencyRotation,
	EpochInfo, QualifyValidator,
};
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
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

		Self::deposit_event(Event::AuctionCompleted(outcome.winners.clone()));

		Ok(outcome)
	}
}

impl<T: Config> Pallet<T> {
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
