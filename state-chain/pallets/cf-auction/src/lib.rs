#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod auction_resolver;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

pub use weights::WeightInfo;

pub use auction_resolver::*;
use cf_traits::{
	AuctionOutcome, Auctioneer, Bid, BidderProvider, Chainflip, EpochInfo, QualifyNode,
};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
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
		/// Qualify an authority
		type AuctionQualification: QualifyNode<ValidatorId = Self::ValidatorId>;
		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
	}

	/// Auction parameters.
	#[pallet::storage]
	#[pallet::getter(fn auction_parameters)]
	pub(super) type AuctionParameters<T: Config> = StorageValue<_, SetSizeParameters, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction has a set of winners \[winners, bond\]
		AuctionCompleted(Vec<T::ValidatorId>, T::Amount),
		/// The auction parameters have been changed \[new_parameters\]
		AuctionParametersChanged(SetSizeParameters),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Auction parameters are invalid.
		InvalidAuctionParameters,
		/// The dynamic set size ranges are inconsistent.
		InconsistentRanges,
		/// Not enough bidders were available to resolve the auction.
		NotEnoughBidders,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the auction parameters.
		///
		/// The dispatch origin of this function must be Governance.
		///
		/// ## Events
		///
		/// - [AuctionParametersChanged](Event::AuctionParametersChanged)
		///
		/// ## Errors
		///
		/// - [InvalidAuctionParameters](Error::InvalidAuctionParameters)
		#[pallet::weight(T::WeightInfo::set_auction_parameters())]
		pub fn set_auction_parameters(
			origin: OriginFor<T>,
			parameters: SetSizeParameters,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			let _ok = Self::try_update_auction_parameters(parameters)?;
			Self::deposit_event(Event::AuctionParametersChanged(parameters));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub min_size: u32,
		pub max_size: u32,
		pub max_expansion: u32,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self { min_size: 3, max_size: 15, max_expansion: 5 }
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			Pallet::<T>::try_update_auction_parameters(SetSizeParameters {
				min_size: self.min_size,
				max_size: self.max_size,
				max_expansion: self.max_expansion,
			})
			.expect("we should provide valid auction parameters at genesis");
		}
	}
}

impl<T: Config> Auctioneer<T> for Pallet<T> {
	type Error = Error<T>;

	fn resolve_auction() -> Result<AuctionOutcome<T::ValidatorId, T::Amount>, Error<T>> {
		let mut bids = T::BidderProvider::get_bidders();
		// Determine if this node is qualified for bidding
		bids.retain(|Bid { bidder_id, .. }| T::AuctionQualification::is_qualified(bidder_id));

		let outcome = SetSizeMaximisingAuctionResolver::try_new(
			T::EpochInfo::current_authority_count(),
			AuctionParameters::<T>::get(),
		)?
		.resolve_auction(bids)?;

		Self::deposit_event(Event::AuctionCompleted(outcome.winners.clone(), outcome.bond));

		Ok(outcome)
	}
}

impl<T: Config> Pallet<T> {
	fn try_update_auction_parameters(new_parameters: SetSizeParameters) -> Result<(), Error<T>> {
		let _ = SetSizeMaximisingAuctionResolver::try_new(
			T::EpochInfo::current_authority_count(),
			new_parameters,
		)?;
		AuctionParameters::<T>::put(new_parameters);
		Ok(())
	}
}
