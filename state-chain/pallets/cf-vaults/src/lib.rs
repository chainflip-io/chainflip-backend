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
mod chains;

use frame_support::pallet_prelude::*;
use cf_traits::{Witnesser, AuctionError, AuctionConfirmation, AuctionEvents, AuctionPenalty, NonceProvider};
pub use pallet::*;
use sp_std::prelude::*;
use crate::rotation::*;
use sp_runtime::DispatchResult;
use sp_runtime::traits::UniqueSaturatedInto;
use frame_support::traits::UnixTime;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use frame_support::traits::UnixTime;
	use codec::FullCodec;
	use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub};

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + ChainFlip + AuctionManager<<Self as ChainFlip>::ValidatorId, <Self as ChainFlip>::Amount> {
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
		/// The Chain for our Vault
		type Chain: Chain<RequestIndex, Self::ValidatorId, RotationError<Self::ValidatorId>>;
		/// The Nonce for this Chain
		type Nonce: Member
			+ FullCodec
			+ Copy
			+ Default
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize
			+ CheckedSub;
		/// Time source in generating the Nonce
		type TimeSource: UnixTime;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
	}

	/// Current request index used in request/response (NB. this should be global and probably doesn't fit this architecture)
	#[pallet::storage]
	#[pallet::getter(fn request_idx)]
	pub(super) type RequestIdx<T: Config<I>, I: 'static = ()> = StorageValue<_, RequestIndex, ValueQuery>;

	/// A map acting as a list of our current vault rotations
	#[pallet::storage]
	pub(super) type VaultRotations<T: Config<I>, I: 'static = ()> = StorageMap<_, Blake2_128Concat, RequestIndex, VaultRotation<RequestIndex, T::ValidatorId>>;

	/// The Vault for this instance
	#[pallet::storage]
	pub(super) type Vault<T: Config<I>, I: 'static = ()> = StorageValue<_, VaultRotationResponse, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Request a key generation \[request_index, request\]
		KeygenRequestEvent(RequestIndex, KeygenRequest<T::ValidatorId>),
		/// Request a rotation of the vault for this chain \[request_index, request\]
		VaultRotationRequest(RequestIndex, VaultRotationRequest),
		/// The vault for the request has rotated \[request_index\]
		VaultRotationCompleted(RequestIndex),
		/// A rotation of vaults has been aborted \[request_indexes\]
		RotationAborted(RequestIndexes),
		/// A complete set of vaults have been rotated
		VaultsRotated,
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// An invalid request idx
		InvalidRequestIdx,
		/// We have an empty validator set
		EmptyValidatorSet,
		/// A vault rotation has failed
		VaultRotationCompletionFailed,
		/// A key generation response has failed
		KeygenResponseFailed,
		/// A vault rotation has failed
		VaultRotationFailed,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {

		// 2/3 threshold from our old validators
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
			match Self::try_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(e.into())
			}
		}

		// 2/3 threshold from our old validators
		#[pallet::weight(10_000)]
		pub fn witness_vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: VaultRotationResponse,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T, I>::vault_rotation_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: VaultRotationResponse,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			Self::try_is_valid(request_id)?;
			match Self::try_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(e.into())
			}
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

impl<T: Config<I>, I: 'static> NonceProvider for Pallet<T, I> {
	type Nonce = T::Nonce;

	fn generate_nonce() -> Self::Nonce {
		// For now, we expect the nonce to be an u64 to stay compatible with the CFE
		let u64_nonce = T::TimeSource::now().as_nanos() as u64;
		u64_nonce.unique_saturated_into()
	}
}

impl<ValidatorId> From<RotationError<ValidatorId>> for DispatchError {
	fn from(err: RotationError<ValidatorId>) -> Self {
		DispatchError::BadOrigin
	}
}

impl<T: Config<I>, I: 'static> From<RotationError<T::ValidatorId>> for Error<T, I> {
	fn from(err: RotationError<T::ValidatorId>) -> Self {
		match err {
			RotationError::EmptyValidatorSet => Error::<T, I>::EmptyValidatorSet,
			_ => Error::<T, I>::KeygenResponseFailed
			// RotationError::InvalidValidators => {}
			// RotationError::BadValidators(_) => {}
			// RotationError::FailedConstruct => {}
			// RotationError::FailedToComplete => {}
			// RotationError::KeygenResponseFailed => {}
			// RotationError::VaultRotationCompletionFailed => {}
		}
	}
}

impl<T: Config<I>, I: 'static> TryIndex<RequestIndex> for Pallet<T, I> {
	fn try_is_valid(idx: RequestIndex) -> DispatchResult {
		ensure!(VaultRotations::<T, I>::contains_key(idx), Error::<T, I>::InvalidRequestIdx);
		Ok(())
	}
}

impl<T: Config<I>, I: 'static> Index<RequestIndex> for Pallet<T, I> {
	fn next() -> RequestIndex {
		RequestIdx::<T, I>::mutate(|idx| *idx + 1)
	}

	fn invalidate(idx: RequestIndex) {
		VaultRotations::<T, I>::remove(idx);
	}

	fn is_empty() -> bool {
		VaultRotations::<T, I>::iter().count() == 0
	}

	fn is_valid(idx: RequestIndex) -> bool {
		VaultRotations::<T, I>::contains_key(idx)
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn new_vault_rotation(keygen_request: KeygenRequest<T::ValidatorId>) -> RequestIndex {
		let idx = Self::next();
		VaultRotations::<T, I>::insert(idx, VaultRotation::new(idx, keygen_request));
		idx
	}

	fn abort_rotation() {
		Self::deposit_event(Event::RotationAborted(VaultRotations::<T, I>::iter().map(|(k, _)| k).collect()));
		VaultRotations::<T, I>::remove_all();
		T::Penalty::abort();
	}
}

impl<T: Config<I>, I: 'static>
	RequestResponse<RequestIndex, KeygenRequest<T::ValidatorId>, KeygenResponse<T::ValidatorId>, RotationError<T::ValidatorId>>
	for Pallet<T, I>
{
	fn try_request(_index: RequestIndex, request: KeygenRequest<T::ValidatorId>) -> Result<(), RotationError<T::ValidatorId>> {
		// Signal to CFE that we are wanting to start a new key generation
		ensure!(!request.validator_candidates.is_empty(), RotationError::EmptyValidatorSet);
		let idx = Self::new_vault_rotation(request.clone());
		Self::deposit_event(Event::KeygenRequestEvent(idx, request));
		Ok(())
	}

	fn try_response(index: RequestIndex, response: KeygenResponse<T::ValidatorId>) -> Result<(), RotationError<T::ValidatorId>> {
		match response {
			KeygenResponse::Success(new_public_key) => {
				// Go forth and construct
				VaultRotations::<T, I>::mutate(index, |maybe_vault_rotation| {
					if let Some(vault_rotation) = maybe_vault_rotation {
						vault_rotation.new_public_key = new_public_key.to_vec();
						let validators = vault_rotation.candidate_validators();
						if validators.is_empty() {
							// If we have no validators then clear this out, this shouldn't happen
							Self::abort_rotation();
							Err(RotationError::EmptyValidatorSet)
						} else {
							T::Chain::try_start_vault_rotation(index, new_public_key, validators.to_vec())
						}
					} else {
						unreachable!("This shouldn't happen but we need to maybe signal this")
					}
				})
			}
			KeygenResponse::Failure(bad_validators) => {
				// Abort this key generation request
				Self::abort_rotation();
				// Do as you wish with these, I wash my hands..
				T::Penalty::penalise(bad_validators);

				Ok(())
			}
		}
	}
}

impl<T: Config<I>, I: 'static> AuctionEvents<T::ValidatorId, T::Amount> for Pallet<T, I> {
	fn on_completed(winners: Vec<T::ValidatorId>, _: T::Amount) -> Result<(), AuctionError>{
		// Create a KeyGenRequest
		let keygen_request = KeygenRequest {
			chain: T::Chain::chain_params(),
			validator_candidates: winners.clone(),
		};

		Self::try_request(Self::next(), keygen_request).map_err(|_| AuctionError::Abort)
	}
}

impl<T: Config<I>, I: 'static> AuctionConfirmation for Pallet<T, I> {
	fn try_confirmation() -> Result<(), AuctionError> {
		if Self::is_empty() {
			// We can now confirm the auction and rotate
			// The process has completed successfully
			Self::deposit_event(Event::VaultsRotated);
			Ok(())
		} else {
			Err(AuctionError::NotConfirmed)
		}
	}
}

impl<T: Config<I>, I: 'static>
	RequestResponse<RequestIndex, VaultRotationRequest, VaultRotationResponse, RotationError<T::ValidatorId>>
	for Pallet<T, I>
{
	fn try_request(index: RequestIndex, request: VaultRotationRequest) -> Result<(), RotationError<T::ValidatorId>>  {
		// Signal to CFE that we are wanting to start the rotation
		Self::deposit_event(Event::VaultRotationRequest(index, request));
		Ok(().into())
	}

	fn try_response(index: RequestIndex, response: VaultRotationResponse) -> Result<(), RotationError<T::ValidatorId>>  {
		// This request is complete
		Self::invalidate(index);
		// Store for this instance the keys
		Vault::<T, I>::set(response);
		Self::deposit_event(Event::VaultRotationCompleted(index));
		Ok(().into())
	}
}

impl<T: Config<I>, I: 'static> ChainEvents<RequestIndex, T::ValidatorId, RotationError<T::ValidatorId>> for Pallet<T, I> {
	fn try_complete_vault_rotation(
		index: RequestIndex,
		result: Result<VaultRotationRequest, RotationError<T::ValidatorId>>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		match result {
			Ok(request) => Self::try_request(index, request),
			Err(_) => {
				// Abort this key generation request
				Self::abort_rotation();
				Err(RotationError::VaultRotationCompletionFailed)
			}
		}
	}
}