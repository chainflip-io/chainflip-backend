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
//! The module contains functionality to run a contest or auction in which a set of bidders are
//! provided via the `BidderProvider` trait.  Calling `Auction::process()` we push forward the state
//! of our auction.
//!
//! First we are looking for bidders in the `AuctionPhase::WaitingForBids` phase in which we
//! validate their suitability for the next phase `AuctionPhase::BidsTaken`.
//! During the `AuctionPhase::BidsTaken` phase we run an auction which selects a list of winners and
//! sets the state to `WinnersSelected` and giving us our winners and the minimum bid.
//! The caller would then finally call `Auction::process()` to finalise the auction, this can only
//! happen on confirmation via the `AuctionConfirmation` trait. From which it would move to
//! `WaitingForBids` for the next auction to be started.
//!
//! At any point in time the auction can be aborted using `Auction::abort()` returning state to
//! `WaitingForBids`.
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

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use cf_traits::{
	Auction, AuctionError, AuctionPhase, AuctionRange, BidderProvider, VaultRotation,
	VaultRotationHandler,
};
use frame_support::pallet_prelude::*;
use frame_support::sp_runtime::offchain::storage_lock::BlockNumberProvider;
use frame_support::sp_std::mem;
use frame_support::traits::ValidatorRegistration;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_std::cmp::min;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::VaultRotation;
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
		type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
		/// An identity for a validator
		type AccountId: Member + Parameter;
		/// Providing bidders
		type BidderProvider: BidderProvider<
			AccountId = <Self as pallet::Config>::AccountId,
			Amount = Self::Amount,
		>;
		/// To confirm we have a session key registered for a validator
		type Registrar: ValidatorRegistration<<Self as pallet::Config>::AccountId>;
		/// An index for the current auction
		type AuctionIndex: Member + Parameter + Default + Add + One + Copy;
		/// Minimum amount of bidders
		#[pallet::constant]
		type MinAuctionSize: Get<u32>;
		/// The lifecycle of our auction
		type Handler: VaultRotation<AccountId = <Self as pallet::Config>::AccountId>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current phase of the auction
	#[pallet::storage]
	#[pallet::getter(fn current_phase)]
	pub(super) type CurrentPhase<T: Config> =
		StorageValue<_, AuctionPhase<<T as pallet::Config>::AccountId, T::Amount>, ValueQuery>;

	/// Size range for number of bidders in auction (min, max)
	#[pallet::storage]
	#[pallet::getter(fn auction_size_range)]
	pub(super) type AuctionSizeRange<T: Config> = StorageValue<_, AuctionRange, ValueQuery>;

	/// The current auction we are in
	#[pallet::storage]
	#[pallet::getter(fn current_auction_index)]
	pub(super) type CurrentAuctionIndex<T: Config> = StorageValue<_, T::AuctionIndex, ValueQuery>;

	/// The set of bad validators
	#[pallet::storage]
	#[pallet::getter(fn bad_validators)]
	pub(super) type BadValidators<T: Config> =
		StorageValue<_, Vec<<T as pallet::Config>::AccountId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[auction_index\]
		AuctionStarted(T::AuctionIndex),
		/// An auction has a set of winners \[auction_index, winners\]
		AuctionCompleted(T::AuctionIndex, Vec<<T as pallet::Config>::AccountId>),
		/// The auction has been confirmed off-chain \[auction_index\]
		AuctionConfirmed(T::AuctionIndex),
		/// Awaiting bidders for the auction
		AwaitingBidders,
		/// The auction range upper limit has changed \[before, after\]
		AuctionRangeChanged(AuctionRange, AuctionRange),
		/// The auction was aborted \[auction_index\]
		AuctionAborted(T::AuctionIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid auction index used in confirmation
		InvalidAuction,
		InvalidRange,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the size of our auction range
		///
		/// The dispatch origin of this function must be root.
		#[pallet::weight(10_000)]
		pub(super) fn set_auction_size_range(
			origin: OriginFor<T>,
			range: AuctionRange,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			match Self::set_auction_range(range) {
				Ok(old) => {
					Self::deposit_event(Event::AuctionRangeChanged(old, range));
					Ok(().into())
				}
				Err(_) => Err(Error::<T>::InvalidRange.into()),
			}
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub auction_size_range: AuctionRange,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				auction_size_range: (Zero::zero(), Zero::zero()),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			AuctionSizeRange::<T>::set(self.auction_size_range);
			// Run through an auction
			if Pallet::<T>::process().and(Pallet::<T>::process()).is_ok() {
				if let Err(err) = Pallet::<T>::process() {
					panic!("Failed to confirm auction: {:?}", err);
				}
			} else {
				panic!("Failed selecting winners in auction");
			}
		}
	}
}

impl<T: Config> Auction for Pallet<T> {
	type AccountId = <T as pallet::Config>::AccountId;
	type Amount = T::Amount;
	type BidderProvider = T::BidderProvider;

	fn auction_range() -> AuctionRange {
		<AuctionSizeRange<T>>::get()
	}

	/// Set new auction range, returning on success the old value
	fn set_auction_range(range: AuctionRange) -> Result<AuctionRange, AuctionError> {
		let (low, high) = range;

		if low == high
			|| low < T::MinAuctionSize::get()
			|| high < T::MinAuctionSize::get()
			|| high < low
		{
			return Err(AuctionError::InvalidRange);
		}

		let old = <AuctionSizeRange<T>>::get();
		if old == range {
			return Err(AuctionError::InvalidRange);
		}

		<AuctionSizeRange<T>>::put(range);
		Ok(old)
	}

	fn phase() -> AuctionPhase<Self::AccountId, Self::Amount> {
		<CurrentPhase<T>>::get()
	}

	fn waiting_on_bids() -> bool {
		mem::discriminant(&Self::phase()) == mem::discriminant(&AuctionPhase::default())
	}

	/// Move our auction process to the next phase returning success with phase completed
	///
	/// At each phase we assess the bidders based on a fixed set of criteria which results
	/// in us arriving at a winning list and a bond set for this auction
	fn process() -> Result<AuctionPhase<Self::AccountId, Self::Amount>, AuctionError> {
		return match <CurrentPhase<T>>::get() {
			// Run some basic rules on what we consider as valid bidders
			// At the moment this includes checking that their bid is more than 0, which
			// shouldn't be possible and whether they have registered their session keys
			// to be able to actual join the validating set.  If we manage to pass these tests
			// we kill the last set of winners stored, set the bond to 0, store this set of
			// bidders and change our state ready for an 'Auction' to be ran
			AuctionPhase::WaitingForBids(_, _) => {
				let mut bidders = T::BidderProvider::get_bidders();
				// Rule #1 - They are not bad
				bidders.retain(|(id, _)| !BadValidators::<T>::get().contains(id));
				// They aren't bad now
				BadValidators::<T>::kill();
				// Rule #2 - If we have a bid at 0 then please leave
				bidders.retain(|(_, amount)| !amount.is_zero());
				// Rule #3 - They are registered
				bidders.retain(|(id, _)| T::Registrar::is_registered(id));
				// Rule #4 - Confirm we have our set size
				if (bidders.len() as u32) < <AuctionSizeRange<T>>::get().0 {
					return Err(AuctionError::MinValidatorSize);
				};

				let phase = AuctionPhase::BidsTaken(bidders);
				<CurrentPhase<T>>::put(phase.clone());

				<CurrentAuctionIndex<T>>::mutate(|idx| *idx + One::one());

				Self::deposit_event(Event::AuctionStarted(<CurrentAuctionIndex<T>>::get()));
				Ok(phase)
			}
			// We sort by bid and cut the size of the set based on auction size range
			// If we have a valid set, within the size range, we store this set as the
			// 'winners' of this auction, change the state to 'Completed' and store the
			// minimum bid needed to be included in the set.
			AuctionPhase::BidsTaken(mut bidders) => {
				if !bidders.is_empty() {
					bidders.sort_unstable_by_key(|k| k.1);
					bidders.reverse();
					let max_size = min(<AuctionSizeRange<T>>::get().1, bidders.len() as u32);
					let bidders = bidders.get(0..max_size as usize);
					if let Some(bidders) = bidders {
						if let Some((_, min_bid)) = bidders.last() {
							let winners: Vec<Self::AccountId> =
								bidders.iter().map(|i| i.0.clone()).collect();
							let phase = AuctionPhase::WinnersSelected(winners.clone(), *min_bid);
							<CurrentPhase<T>>::put(phase.clone());

							Self::deposit_event(Event::AuctionCompleted(
								<CurrentAuctionIndex<T>>::get(),
								winners.clone(),
							));

							T::Handler::start_vault_rotation(winners)
								.map_err(|_| AuctionError::Abort)?;
							return Ok(phase);
						}
					}
				}

				return Err(AuctionError::Empty);
			}
			// Things have gone well and we have a set of 'Winners', congratulations.
			// We are ready to call this an auction a day resetting the bidders in storage and
			// setting the state ready for a new set of 'Bidders'
			AuctionPhase::WinnersSelected(winners, min_bid) => {
				// If this is genesis we auto confirm
				let result = if frame_system::Pallet::<T>::current_block_number() == Zero::zero() {
					Ok(())
				} else {
					T::Handler::finalize_rotation()
				};

				match result {
					Ok(_) => {
						let phase = AuctionPhase::WaitingForBids(winners, min_bid);
						<CurrentPhase<T>>::put(phase.clone());
						Self::deposit_event(Event::AuctionConfirmed(
							CurrentAuctionIndex::<T>::get(),
						));
						Self::deposit_event(Event::AwaitingBidders);
						Ok(phase)
					}
					Err(_) => Err(AuctionError::NotConfirmed),
				}
			}
		};
	}
}

impl<T: Config> VaultRotationHandler for Pallet<T> {
	type AccountId = <T as pallet::Config>::AccountId;

	fn abort() {
		<CurrentPhase<T>>::put(AuctionPhase::default());
		Self::deposit_event(Event::AuctionAborted(<CurrentAuctionIndex<T>>::get()));
	}

	fn penalise(bad_validators: Vec<Self::AccountId>) {
		BadValidators::<T>::set(bad_validators);
	}
}
