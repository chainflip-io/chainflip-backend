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

	/// Auction parameters.
	#[pallet::storage]
	#[pallet::getter(fn auction_parameters)]
	pub(super) type AuctionParameters<T: Config> =
		StorageValue<_, <ResolverV1<T> as AuctionResolver<T>>::AuctionParameters, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction has a set of winners \[winners, bond\]
		AuctionCompleted(Vec<T::ValidatorId>, T::Amount),
		/// The active validator range upper limit has changed \[before, after\]
		AuctionParametersChanged(
			<ResolverV1<T> as AuctionResolver<T>>::AuctionParameters,
			<ResolverV1<T> as AuctionResolver<T>>::AuctionParameters,
		),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Auction parameters are invalid.
		InvalidAuctionParameters,
		/// Not enough bidders were available to resolve the auction.
		NotEnoughBidders,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the auction parameters.
		///
		/// The dispatch origin of this function must be root.
		///
		/// ## Events
		///
		/// - [AuctionParametersChanged](Event::AuctionParametersChanged)
		///
		/// ## Errors
		///
		/// - [InvalidAuctionParameters](Error::InvalidAuctionParameters)
		#[pallet::weight(T::WeightInfo::set_active_validator_range())]
		pub fn set_active_validator_range(
			origin: OriginFor<T>,
			active_validator_range: (u32, u32),
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let new = AuctionParametersV1 {
				min_size: active_validator_range.0,
				max_size: active_validator_range.1,
				active_to_backup_validator_ratio: T::ActiveToBackupValidatorRatio::get(),
				percentage_of_backup_validators_in_emergency:
					T::PercentageOfBackupValidatorsInEmergency::get(),
			};
			let old = Self::set_auction_parameters(new)?;
			Self::deposit_event(Event::AuctionParametersChanged(old, new));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub min_size: u32,
		pub max_size: u32,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self { min_size: 3, max_size: 15 }
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			Pallet::<T>::set_auction_parameters(AuctionParametersV1 {
				min_size: self.min_size,
				max_size: self.max_size,
				active_to_backup_validator_ratio: T::ActiveToBackupValidatorRatio::get(),
				percentage_of_backup_validators_in_emergency:
					T::PercentageOfBackupValidatorsInEmergency::get(),
			})
			.expect("we should provide valid auction parameters at genesis");
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

		let outcome = ResolverV1::resolve_auction(
			AuctionParameters::<T>::get(),
			AuctionContextV1 {
				is_emergency: T::EmergencyRotation::emergency_rotation_in_progress(),
			},
			bids,
		)?;

		Self::deposit_event(Event::AuctionCompleted(outcome.winners.clone(), outcome.bond));

		Ok(outcome)
	}
}

impl<T: Config> Pallet<T> {
	fn set_auction_parameters(
		auction_parameters: AuctionParametersV1,
	) -> Result<AuctionParametersV1, Error<T>> {
		let (low, high) = (auction_parameters.min_size, auction_parameters.max_size);
		ensure!(low <= high, Error::<T>::InvalidAuctionParameters);
		ensure!(
			high >= low && low >= T::MinValidators::get(),
			Error::<T>::InvalidAuctionParameters
		);
		let old = AuctionParameters::<T>::get();
		if old != auction_parameters {
			AuctionParameters::<T>::put(auction_parameters);
		}
		Ok(old)
	}
}
