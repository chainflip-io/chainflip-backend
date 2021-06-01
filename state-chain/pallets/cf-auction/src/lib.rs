#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Auction Module
//!
//! A module to manage auctions for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality to run a contest or auction in which a set of
// bidders are provided via the `BidderProvider` trait.  Calling `process()` we push forward the
// state of our auction.  First we are looking for `Bidders` with which we validate their suitability
// for the next phase `Auction`.  During this phase we run an auction which selects a list of winners
// sets a minimum bid of what was need to get in the winning list and set the state to `Completed`.
// The caller would then finally call `process()` to clear the auction in which it would move to
// `Bidders` waiting for the next auction to be started.
//!
//! ## Terminology
//! - **Bidder:** An entity that has placed a bid and would hope to be included in the winning set
//! - **Winners:** Those bidders that have been evaluated and have been included in the the winning set
//! - **Minimum Bid:** The minimum bid required to be included in the Winners set
//! - **Auction Range:** A range specifying the minimum number of bidders we require and an upper range
//!	  specifying the maximum size for the winning set

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Zero, One};
use sp_std::prelude::*;
use frame_support::pallet_prelude::*;
use frame_support::traits::ValidatorRegistration;
use sp_std::cmp::min;
use cf_traits::{Auction, AuctionPhase, AuctionError, BidderProvider, Bid, AuctionRange};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use frame_support::traits::ValidatorRegistration;
	use sp_std::ops::Add;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// An amount for a bid
		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
		/// An identity for a validator
		type ValidatorId: Member + Parameter;
		/// Providing bidders
		type BidderProvider: BidderProvider<ValidatorId=Self::ValidatorId, Amount=Self::Amount>;
		/// To confirm we have a session key registered for a validator
		type Registrar: ValidatorRegistration<Self::ValidatorId>;
		/// An index for the current auction
		type AuctionIndex: Member + Parameter + Default + Add + One + Copy;
		/// Minimum amount of bidders
		#[pallet::constant]
		type MinAuctionSize: Get<u32>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current phase of the auction
	#[pallet::storage]
	#[pallet::getter(fn current_phase)]
	pub(super) type CurrentPhase<T: Config> = StorageValue<_, AuctionPhase, ValueQuery>;

	/// The minimum bid required to be in the winning set
	#[pallet::storage]
	#[pallet::getter(fn minimum_bid)]
	pub(super) type MinimumBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// The list of current bidders for the auction
	#[pallet::storage]
	#[pallet::getter(fn bidders)]
	pub(super) type Bidders<T: Config> = StorageValue<_, Vec<(T::ValidatorId, T::Amount)>, ValueQuery>;

	/// The list of our winners for this auction
	#[pallet::storage]
	#[pallet::getter(fn winners)]
	pub(super) type Winners<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// Size range for number of bidders in auction (min, max)
	#[pallet::storage]
	#[pallet::getter(fn auction_size_range)]
	pub(super) type AuctionSizeRange<T: Config> = StorageValue<_, AuctionRange, ValueQuery>;

	/// The current auction we are in
	#[pallet::storage]
	#[pallet::getter(fn current_auction_index)]
	pub(super) type CurrentAuctionIndex<T: Config> = StorageValue<_, T::AuctionIndex, ValueQuery>;

	/// The auction we are waiting for confirmation
	#[pallet::storage]
	#[pallet::getter(fn auction_to_confirm)]
	pub(super) type AuctionToConfirm<T: Config> = StorageValue<_, T::AuctionIndex, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[auction_index\]
		AuctionStarted(T::AuctionIndex),
		/// An auction has a set of winners
		AuctionCompleted(T::AuctionIndex),
		/// The auction has been confirmed off-chain \[auction_index\]
		AuctionConfirmed(T::AuctionIndex),
		/// Awaiting bidders for the auction
		AwaitingBidders,
		/// The auction range upper limit has changed \[before, after\]
		AuctionRangeChanged(AuctionRange, AuctionRange),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid auction index used in confirmation
		InvalidAuction,
		InvalidRange,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(
			10_000
		)]
		pub(super) fn confirm_auction(
			origin: OriginFor<T>,
			index: T::AuctionIndex,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(Some(index) == AuctionToConfirm::<T>::get(), Error::<T>::InvalidAuction);
			AuctionToConfirm::<T>::set(None);
			Self::deposit_event(Event::AuctionConfirmed(index));
			Ok(().into())
		}

		/// Sets the size of our auction range
		///
		/// The dispatch origin of this function must be root.
		#[pallet::weight(
			10_000
		)]
		pub(super) fn set_auction_size_range(
			origin: OriginFor<T>,
			range: AuctionRange,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			match Self::set_auction_range(range) {
				Ok(old) => {
					Self::deposit_event(Event::AuctionRangeChanged(old, range));
					Ok(().into())
				},
				Err(_) => {
					Err(Error::<T>::InvalidRange.into())
				},
			}
		}
	}
}

impl<T: Config> Auction for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type BidderProvider = T::BidderProvider;

	fn auction_range() -> AuctionRange {
		<AuctionSizeRange<T>>::get()
	}

	/// Set new auction range, returning on success the old value
	fn set_auction_range(range: AuctionRange) -> Result<AuctionRange, AuctionError> {
		let (low, high) = range;

		if low == high || low < T::MinAuctionSize::get() || high < T::MinAuctionSize::get() {
			return Err(AuctionError::InvalidRange);
		}

		let old = <AuctionSizeRange<T>>::get();
		if old == range {
			return Err(AuctionError::InvalidRange);
		}

		<AuctionSizeRange<T>>::put(range);
		Ok(old)
	}

	fn phase() -> AuctionPhase { <CurrentPhase<T>>::get() }

	/// Move our auction process to the next phase returning success with phase completed
	///
	/// At each phase we assess the bidders based on a fixed set of criteria which results
	/// in us arriving at a winning list and a bond set for this auction
	fn process() -> Result<AuctionPhase, AuctionError> {

		return match <CurrentPhase<T>>::get() {
			// Run some basic rules on what we consider as valid bidders
			// At the moment this includes checking that their bid is more than 0, which
			// shouldn't be possible and whether they have registered their session keys
			// to be able to actual join the validating set.  If we manage to pass these tests
			// we kill the last set of winners stored, set the bond to 0, store this set of
			// bidders and change our state ready for an 'Auction' to be ran
			AuctionPhase::Bidders => {
				let mut bidders = T::BidderProvider::get_bidders();
				// Rule #1 - If we have a bid at 0 then please leave
				bidders.retain(|(_, amount)| !amount.is_zero());
				// Rule #2 - They are registered
				bidders.retain(|(id, _)| T::Registrar::is_registered(id));
				// Rule #3 - Confirm we have our set size
				if (bidders.len() as u32) < <AuctionSizeRange<T>>::get().0 {
					return Err(AuctionError::MinValidatorSize)
				};

				<Winners<T>>::kill();
				<MinimumBid<T>>::kill();
				<Bidders<T>>::put(bidders);
				<CurrentPhase<T>>::put(AuctionPhase::Auction);

				<CurrentAuctionIndex<T>>::mutate(|idx| {
					*idx + One::one()
				});

				<AuctionToConfirm::<T>>::put(<CurrentAuctionIndex<T>>::get());

				Self::deposit_event(Event::AuctionStarted(<CurrentAuctionIndex<T>>::get()));

				Ok(AuctionPhase::Bidders)
			},
			// We sort by bid and cut the size of the set based on auction size range
			// If we have a valid set, within the size range, we store this set as the
			// 'winners' of this auction, change the state to 'Completed' and store the
			// minimum bid needed to be included in the set.
			AuctionPhase::Auction => {
				let mut bidders = <Bidders<T>>::get();
				if !bidders.is_empty() {
					bidders.sort_unstable_by_key(|k| k.1);
					bidders.reverse();
					let max_size = min(<AuctionSizeRange<T>>::get().1, bidders.len() as u32);
					let bidders = bidders.get(0..max_size as usize);
					if let Some(bidders) = bidders {
						if let Some((_, min_bid)) = bidders.last() {
							let winners: Vec<T::ValidatorId> = bidders.iter().map(|i| i.0.clone()).collect();

							<MinimumBid<T>>::put(min_bid);
							<Winners<T>>::put(winners);
							<CurrentPhase<T>>::put(AuctionPhase::Completed);

							<CurrentAuctionIndex<T>>::mutate(|idx| *idx + One::one());
							Self::deposit_event(Event::AuctionCompleted(<CurrentAuctionIndex<T>>::get()));

							return Ok(AuctionPhase::Auction);
						}
					}
				}

				return Err(AuctionError::Empty);
			},
			// Things have gone well and we have a set of 'Winners', congratulations.
			// We are ready to call this an auction a day resetting the bidders in storage and
			// setting the state ready for a new set of 'Bidders'
			AuctionPhase::Completed => {
				if <AuctionToConfirm::<T>>::get().is_some() {
					return Err(AuctionError::NotConfirmed);
				}

				<Bidders<T>>::kill();
				<CurrentPhase<T>>::put(AuctionPhase::Bidders);
				Self::deposit_event(Event::AwaitingBidders);

				Ok(AuctionPhase::Completed)
			}
		};
	}

	fn bidders() -> Vec<Bid<Self>> {
		<Bidders<T>>::get()
	}

	fn winners() -> Vec<Self::ValidatorId> {
		<Winners<T>>::get()
	}

	fn minimum_bid() -> Self::Amount {
		<MinimumBid<T>>::get()
	}
}