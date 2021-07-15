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
		EmptyValidatorSet,
		VaultRotationCompletionFailed,
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
			Self::try_is_valid(request_id)?;
			Self::try_response(request_id, response)
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
			Self::try_is_valid(request_id)?;
			Self::try_response(request_id, response)
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

impl<T: Config<I>, I: 'static> TryIndex<RequestIndex> for Pallet<T, I> {
	fn try_is_valid(idx: RequestIndex) -> DispatchResultWithPostInfo {
		ensure!(VaultRotations::<T, I>::contains_key(idx), Error::<T, I>::InvalidRequestIdx);
		Ok(().into())
	}
}

impl<T: Config<I>, I: 'static> Index<RequestIndex> for Pallet<T, I> {
	fn next() -> RequestIndex {
		let idx = RequestIdx::<T, I>::mutate(|idx| *idx + 1);
		idx
	}

	fn clear(idx: RequestIndex) {
		VaultRotations::<T, I>::remove(idx);
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn new_vault_rotation(keygen_request: KeygenRequest<T::ValidatorId>) -> RequestIndex {
		let idx = Self::next();
		VaultRotations::<T, I>::insert(idx, VaultRotation::new(idx, keygen_request));
		idx
	}
}

impl<T: Config<I>, I: 'static>
	RequestResponse<RequestIndex, KeygenRequest<T::ValidatorId>, KeygenResponse<T::ValidatorId>>
	for Pallet<T, I>
{
	fn try_request(_index: RequestIndex, request: KeygenRequest<T::ValidatorId>) -> DispatchResultWithPostInfo {
		// Signal to CFE that we are wanting to start a new key generation
		ensure!(!request.validator_candidates.is_empty(), Error::<T, I>::EmptyValidatorSet);
		let idx = Self::new_vault_rotation(request.clone());
		Self::deposit_event(Event::KeygenRequestEvent(idx, request));
		Ok(().into())
	}

	fn try_response(index: RequestIndex, response: KeygenResponse<T::ValidatorId>) -> DispatchResultWithPostInfo {
		match response {
			KeygenResponse::Success(new_public_key) => {
				// Go forth and construct
				VaultRotations::<T, I>::mutate(index, |maybe_vault_rotation| {
					if let Some(vault_rotation) = maybe_vault_rotation {
						vault_rotation.new_public_key = new_public_key.to_vec();
						let validators = vault_rotation.candidate_validators();
						if validators.is_empty() {
							// If we have no validators then clear this out, this shouldn't happen
							Self::clear(index);
							Err(Error::<T, I>::EmptyValidatorSet.into())
						} else {
							T::Constructor::try_start_construction_phase(index, new_public_key, validators.to_vec())
						}
					} else {
						unreachable!("This shouldn't happen but we need to maybe signal this")
					}
				})
			}
			KeygenResponse::Failure(bad_validators) => {
				// Abort this key generation request
				Self::clear(index);
				// Do as you wish with these, I wash my hands..
				T::Reporter::penalise(bad_validators);

				Ok(().into())
			}
		}
	}
}

impl<T: Config<I>, I: 'static>
	RequestResponse<RequestIndex, ValidatorRotationRequest, ValidatorRotationResponse>
	for Pallet<T, I>
{
	fn try_request(index: RequestIndex, request: ValidatorRotationRequest) -> DispatchResultWithPostInfo {
		// Signal to CFE that we are wanting to start the rotation
		Self::deposit_event(Event::ValidatorRotationRequest(index, request));
		Ok(().into())
	}

	fn try_response(index: RequestIndex, response: ValidatorRotationResponse) -> DispatchResultWithPostInfo {
		// This request is complete
		Self::clear(index);
		// We can now confirm the auction and rotate
		T::Confirmation::set_awaiting_confirmation(false);
		// The process has completed successfully
		Self::deposit_event(Event::VaultRotationCompleted(index));
		Ok(().into())
	}
}

impl<T: Config<I>, I: 'static> ConstructHandler<RequestIndex, T::ValidatorId> for Pallet<T, I> {
	fn try_on_completion(
		index: RequestIndex,
		result: Result<ValidatorRotationRequest, ValidatorRotationError<T::ValidatorId>>,
	) -> DispatchResultWithPostInfo {
		match result {
			Ok(request) => {
				Self::try_request(index, request)
			}
			Err(_) => {
				todo!("can we use this even?");
				// Abort this key generation request
				Self::clear(index);

				Err(Error::<T, I>::VaultRotationCompletionFailed.into())
			}
		}
	}
}