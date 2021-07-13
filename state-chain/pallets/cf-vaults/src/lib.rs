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
mod rotation;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use frame_support::pallet_prelude::*;
use frame_support::sp_std::mem;
use frame_support::traits::ValidatorRegistration;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_std::prelude::*;

type RequestIdx = u32;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use crate::rotation::*;

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
		/// Our constructor
		type Constructor: Construct<Self::ValidatorId>;
		/// Our constructor handler
		type ConstructorHandler: ConstructionHandler;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		// Request a KGC from the CFE
		// request_id - a unique indicator of this request and should be used throughout the lifetime of this request
		// request - our keygen request
		KeygenRequestEvent(RequestIdx, KeygenRequest<T::ValidatorId>),
		// Request a rotation
		ValidatorRotationRequest(RequestIdx, ValidatorRotationRequest),
		// The validator set has been rotated
		VaultRotationCompleted(RequestIdx),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid request idx
		InvalidRequestIdx,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub(super) fn call_me(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		// A response back from the CFE, good or bad but never ugly.
		#[pallet::weight(10_000)]
		pub fn keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIdx,
			response: KeygenResponse<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		// We have witnessed a rotation, my eyes!
		#[pallet::weight(10_000)]
		pub fn witness_vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIdx,
			response: ValidatorRotationResponse,
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {}
	}
}
