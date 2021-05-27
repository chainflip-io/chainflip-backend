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
//!
//! ## Terminology
//!
//! ### Dispatchable Functions
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::prelude::*;
use log::{debug};
use frame_support::pallet_prelude::*;
use frame_support::traits::ValidatorRegistration;
use sp_std::cmp::min;

#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug)]
pub enum AuctionPhase {
	Bidders,
	Auction,
	Completed
}

impl Default for AuctionPhase {
	fn default() -> Self {
		AuctionPhase::Bidders
	}
}

trait Auction {
	type ValidatorId;
	fn next_phase() -> Result<AuctionPhase, AuctionError>;
}

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AuctionError {
	BondIsZero,
	Empty,
	MinValidatorSize,
}

pub trait BidderProvider<ValidatorId, Amount> {
	fn get_bidders() -> Vec<(ValidatorId, Amount)>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use frame_support::traits::ValidatorRegistration;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
		type ValidatorId: Member + Parameter;
		type BidderProvider: BidderProvider<Self::ValidatorId, Self::Amount>;
		type Registrar: ValidatorRegistration<Self::ValidatorId>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current phase
	#[pallet::storage]
	pub(super) type CurrentPhase<T: Config> = StorageValue<_, AuctionPhase, ValueQuery>;

	/// Current bond value
	#[pallet::storage]
	pub(super) type CurrentBond<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// The working list of bidders
	#[pallet::storage]
	pub(super) type Bidders<T: Config> = StorageValue<_, Vec<(T::ValidatorId, T::Amount)>, ValueQuery>;

	#[pallet::storage]
	pub(super) type Winners<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// Size range for bidders (min, max)
	#[pallet::storage]
	pub(super) type BidderSizeRange<T: Config> = StorageValue<_, (u32, u32), ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

impl<T: Config> Auction for Pallet<T> {
	type ValidatorId = ();

	// This would be called to move to the next phase or continue in the current phase
	fn next_phase() -> Result<AuctionPhase, AuctionError> {
		match <CurrentPhase<T>>::get() {
			AuctionPhase::Bidders => {
				let mut bidders = T::BidderProvider::get_bidders();
				// Rule #1 - If we have a stake at 0 then please leave
				bidders.retain(|(_, amount)| !amount.is_zero());
				// Rule #2 - They are registered
				bidders.retain(|(id, _)| T::Registrar::is_registered(id));
				// Rule #3 - Confirm we have our set size, TODO
				if (bidders.len() as u32) < <BidderSizeRange<T>>::get().0 {
					return Err(AuctionError::MinValidatorSize)
				};

				<Winners<T>>::kill();
				<Bidders<T>>::put(bidders);
				let phase = AuctionPhase::Auction;
				<CurrentPhase<T>>::put(phase);

				return Ok(phase);
			},
			AuctionPhase::Auction => {
				let mut bidders = <Bidders<T>>::get();

				if !bidders.is_empty() {
					bidders.sort_unstable_by_key(|k| k.1);
					bidders.reverse();
					let max_size = min(<BidderSizeRange<T>>::get().1, bidders.len() as u32);
					let bidders = bidders.get(0..max_size as usize);
					if let Some(bidders) = bidders {
						if let Some((_, bond)) = bidders.last() {
							let winners: Vec<T::ValidatorId> = bidders.iter().map(|i| i.0.clone()).collect();

							<Winners<T>>::put(winners);
							<Bidders<T>>::kill();
							let phase = AuctionPhase::Completed;
							<CurrentPhase<T>>::put(phase);

							return Ok(phase);
						}
					}
				}

				return Err(AuctionError::Empty);
			},
			AuctionPhase::Completed => {
				let phase = AuctionPhase::Bidders;
				<CurrentPhase<T>>::put(phase);
			}
		}

		Ok(<CurrentPhase<T>>::get())
	}
}