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
		type Constructor: Construct<RequestIndex, Self::ValidatorId>;
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
	#[pallet::getter(fn request_idx)]
	pub(super) type RequestIdx<T: Config> = StorageValue<_, RequestIndex, ValueQuery>;

	#[pallet::storage]
	pub(super) type VaultRotations<T: Config> = StorageMap<_, Blake2_128Concat, RequestIndex, VaultRotation<RequestIndex, T::ValidatorId>>;

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

		// Does this need to be more than 2/3?
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
			ensure!(Self::is_valid(request_id), Error::<T>::InvalidRequestIdx);
			Self::process_response(response);
			Ok(().into())
		}

		// We have witnessed a rotation, my eyes!
		// Assumption here is that it is 2/3 threshold
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
			ensure!(Self::is_valid(request_id), Error::<T>::InvalidRequestIdx);
			Self::process_response(response);
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

impl<T: Config> Index<RequestIndex> for Pallet<T> {
	fn is_valid(idx: RequestIndex) -> bool {
		VaultRotations::<T>::contains_key(idx)
	}

	fn next() -> RequestIndex {
		let idx = RequestIdx::<T>::mutate(|idx| *idx + 1);
		VaultRotations::<T>::insert(idx, VaultRotation::new(idx));
		idx
	}

	fn clear(idx: RequestIndex) {
		VaultRotations::<T>::remove(idx);
	}
}

impl<T: Config> RequestResponse<RequestIndex, KeygenRequest<T::ValidatorId>, KeygenResponse<T::ValidatorId>> for Pallet<T> {
	fn process_request(index: RequestIndex, request: KeygenRequest<T::ValidatorId>) {
		// Signal to CFE that we are wanting to start a new key generation
		Self::deposit_event(Event::KeygenRequestEvent(Self::next(), request));
	}

	fn process_response(index: RequestIndex, response: KeygenResponse<T::ValidatorId>) {
		match response {
			KeygenResponse::Success(_) => {
				// Go forth and construct
				T::Constructor::start_construction_phase(index, response);
			}
			KeygenResponse::Failure(bad_validators) => {
				// Abort this key generation request
				Self::clear(index);
				// Do as you wish with these, I wash my hands..
				T::AuctionPenalty::penalise(bad_validators);
			}
		}
	}
}

impl<T: Config> RequestResponse<RequestIndex, ValidatorRotationRequest, ValidatorRotationResponse> for Pallet<T> {
	fn process_request(index: RequestIndex, request: ValidatorRotationRequest) {
		todo!()
	}

	fn process_response(index: RequestIndex, response: ValidatorRotationResponse) {
		todo!()
	}
}

impl<T: Config> ConstructionManager<RequestIndex> for Pallet<T> {
	fn on_completion(index: RequestIndex, err: bool) {
		todo!()
	}
}

impl<T: Config> KeyRotation<T::ValidatorId> for Pallet<T> {
	type AuctionPenalty = T::AuctionPenalty;
	type KeyGeneration = Self;
	type Construct = Constructor<KeygenResponse<T::ValidatorId>>;
	type ConstructionManager = ConstructionHandler;
	type Rotation = Self;
}
