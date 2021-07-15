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
mod ethereum;

use frame_support::pallet_prelude::*;
use cf_traits::Witnesser;
pub use pallet::*;
use sp_std::prelude::*;
use crate::rotation::*;
use cf_traits::AuctionConfirmation;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + ChainFlip + AuctionManager<<Self as ChainFlip>::ValidatorId> {
		/// The event type
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::Call>;
		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;
		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<
			Call = <Self as pallet::Config<I>>::Call,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		/// Our constructor
		type Constructor: Construct<RequestIndex, Self::ValidatorId>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
	}

	#[pallet::storage]
	#[pallet::getter(fn request_idx)]
	pub(super) type RequestIdx<T: Config<I>, I: 'static = ()> = StorageValue<_, RequestIndex, ValueQuery>;

	#[pallet::storage]
	pub(super) type VaultRotations<T: Config<I>, I: 'static = ()> = StorageMap<_, Blake2_128Concat, RequestIndex, VaultRotation<RequestIndex, T::ValidatorId>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
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
	pub enum Error<T, I = ()> {
		/// An invalid request idx
		InvalidRequestIdx,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {

		// Does this need to be more than 2/3?
		#[pallet::weight(10_000)]
		pub fn witness_keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: KeygenResponse<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T, I>::keygen_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: KeygenResponse<T::ValidatorId>,

		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure!(Self::is_valid(request_id), Error::<T, I>::InvalidRequestIdx);
			Self::process_response(request_id, response);
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
			let call = Call::<T, I>::vault_rotation_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: ValidatorRotationResponse,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure!(Self::is_valid(request_id), Error::<T, I>::InvalidRequestIdx);
			Self::process_response(request_id, response);
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
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig {
		fn build(&self) {}
	}
}

impl<T: Config<I>, I: 'static> Index<RequestIndex> for Pallet<T, I> {
	fn is_valid(idx: RequestIndex) -> bool {
		VaultRotations::<T, I>::contains_key(idx)
	}

	fn next() -> RequestIndex {
		let idx = RequestIdx::<T, I>::mutate(|idx| *idx + 1);
		VaultRotations::<T, I>::insert(idx, VaultRotation::new(idx));
		idx
	}

	fn clear(idx: RequestIndex) {
		VaultRotations::<T, I>::remove(idx);
	}
}

impl<T: Config<I>, I: 'static> RequestResponse<RequestIndex, KeygenRequest<T::ValidatorId>, KeygenResponse<T::ValidatorId>> for Pallet<T, I> {
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

impl<T: Config<I>, I: 'static> RequestResponse<RequestIndex, ValidatorRotationRequest, ValidatorRotationResponse> for Pallet<T, I> {
	fn process_request(index: RequestIndex, request: ValidatorRotationRequest) {
		// Signal to CFE that we are wanting to start the rotation
		Self::deposit_event(Event::ValidatorRotationRequest(index, request));
	}

	fn process_response(index: RequestIndex, response: ValidatorRotationResponse) {
		// This request is complete
		Self::clear(index);
		// We can now confirm the auction and rotate
		T::AuctionConfirmation::set_awaiting_confirmation(false);
		// The process has completed successfully
		Self::deposit_event(Event::VaultRotationCompleted(index));
	}
}

impl<T: Config<I>, I: 'static> ConstructionManager<RequestIndex> for Pallet<T, I> {
	fn on_completion(index: RequestIndex, result: Result<ValidatorRotationRequest, ValidatorRotationError>) {
		match result {
			Ok(request) => {
				Self::process_request(index, request);
			}
			Err(_) => { //TODO can we use this even?
				// Abort this key generation request
				Self::clear(index);
			}
		}
	}
}