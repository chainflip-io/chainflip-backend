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
use cf_traits::Witnesser;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_std::prelude::*;
use crate::rotation::*;
use std::ops::Add;

type RequestIndex = u32;

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
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self>> + IsType<<Self as frame_system::Config>::Call>;
		/// An amount for a bid
		type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
		/// An identity for a validator
		type ValidatorId: Member + Parameter;
		/// Our constructor
		type Constructor: Construct<Self::ValidatorId>;
		/// Our constructor handler
		type ConstructorHandler: ConstructionManager;
		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;
		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<
			Call = <Self as Config>::Call,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		type AuctionPenalty: AuctionPenalty<Self::ValidatorId>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::storage]
	#[pallet::getter(fn auction_size_range)]
	pub(super) type RequestIdx<T: Config> = StorageValue<_, RequestIndex, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		// Request a KGC from the CFE
		// request_id - a unique indicator of this request and should be used throughout the lifetime of this request
		// request - our keygen request
		KeygenRequestEvent(RequestIndex, KeygenRequest<T::ValidatorId>),
		// Request a rotation
		ValidatorRotationRequest(RequestIndex, ValidatorRotationRequest),
		// The validator set has been rotated
		VaultRotationCompleted(RequestIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid request idx
		InvalidRequestIdx,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::weight(10_000)]
		pub fn witness_keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: KeygenResponse<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T>::keygen_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: KeygenResponse<T::ValidatorId>,

		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure!(RequestIdx::<T>::get().is_valid(request_id), Error::<T>::InvalidRequestIdx);
			Ok(().into())
		}

		// We have witnessed a rotation, my eyes!
		#[pallet::weight(10_000)]
		pub fn witness_vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: ValidatorRotationResponse,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T>::vault_rotation_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: ValidatorRotationResponse,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure!(RequestIdx::<T>::get().is_valid(request_id), Error::<T>::InvalidRequestIdx);
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

impl IncrementingIndex for RequestIndex {
	fn is_valid(&self, idx: Self) -> bool {
		*self == idx
	}

	fn next(&mut self) -> Self {
		*self = self.add(1);
		*self
	}
}

impl<T: Config> RequestResponse<KeygenRequest<T::ValidatorId>, KeygenResponse<T::ValidatorId>> for Pallet<T> {
	fn request(&self, request: KeygenRequest<T::ValidatorId>) {
		todo!()
	}

	fn response(&self, response: KeygenResponse<T::ValidatorId>) {
		todo!()
	}
}

impl<T: Config> RequestResponse<ValidatorRotationRequest, ValidatorRotationResponse> for Pallet<T> {
	fn request(&self, request: ValidatorRotationRequest) {
		todo!()
	}

	fn response(&self, response: ValidatorRotationResponse) {
		todo!()
	}
}

pub struct Constructor<ValidatorId> { marker: PhantomData<ValidatorId> }
impl<ValidatorId> Construct<ValidatorId> for Constructor<ValidatorId> {
	fn start_construction_phase(response: KeygenResponse<ValidatorId>) {
		todo!()
	}
}

pub struct ConstructionHandler;

impl ConstructionManager for ConstructionHandler {
}

impl<T: Config> KeyRotation<T::ValidatorId> for Pallet<T> {
	type AuctionPenalty = T::AuctionPenalty;
	type KeyGeneration = Self;
	type Construct = Constructor<KeygenResponse<T::ValidatorId>>;
	type ConstructionManager = ConstructionHandler;
	type Rotation = Self;
}
