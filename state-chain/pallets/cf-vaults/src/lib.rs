#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Pallets Module
//!
//! A module to manage vaults for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//!
//! ## Terminology
//! - **Vault:** An entity

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use cf_traits::{
	Auction, AuctionConfirmation, AuctionError, AuctionPhase, AuctionRange, BidderProvider,
};
use frame_support::pallet_prelude::*;
use frame_support::sp_std::mem;
use frame_support::traits::ValidatorRegistration;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_std::cmp::min;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

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
		type ValidatorId: Member + Parameter;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Nothing has happened
		Nothing(),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Unknown
		UnknownError,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub(super) fn call_me(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
		}
	}
}
