#![cfg_attr(not(feature = "std"), no_std)]

//! # ChainFlip Vaults Module
//!
//! A module managing the vaults of ChainFlip
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality to manage the vault rotation that has to occur for the ChainFlip
//! validator set to rotate.  The process of vault rotation us triggered by a successful auction via
//! the trait `AuctionEvents`, implemented by the `Auction` pallet, which provides a list of suitable
//! validators with which we would like to proceed in rotating the vaults concerned.
//! A key generation request is created for each chain supported and emitted as an event from which
//! a ceremony is performed and on success reports back with a response which is delegated to the chain
//! specialisation which continues performing steps necessary to rotate its vault.  On completing this
//! and calling back to the `Vaults` pallet, via the trait `ChainEvents`, the final step is executed
//! with a vault rotation request being emitted and on success the vault being rotated.
//!
//! ## Terminology
//! - **Vault:** A cryptocurrency wallet.
//! - **Validators:** A set of nodes that validate and support the ChainFlip network.
//! - **Bad Validators:** A set of nodes that have acted badly, the determination of what bad is is
//!   outside the scope of the `Vaults` pallet.
//! - **Key generation:** The process of creating a new key pair which would be used for operating a vault.
//! - **Auction:** A process by which a set of validators are proposed and on successful vault rotation
//!   become the next validating set for the network.
//! - **Vault Rotation:** The rotation of vaults where funds are 'moved' from one to another.
//! - **Validator Rotation:** The rotation of validators from old to new.

use frame_support::pallet_prelude::*;
use sp_runtime::traits::One;
use sp_runtime::DispatchResult;
use sp_std::prelude::*;

use cf_traits::{
	AuctionError, AuctionHandler, AuctionPenalty, NonceProvider, Witnesser,
};
pub use pallet::*;

use crate::rotation::*;

mod rotation;

mod chains;
#[cfg(test)]
mod mock;
pub mod nonce;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::AtLeast32BitUnsigned;

	use super::*;
	use sp_std::ops::Add;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ ChainFlip
		+ AuctionManager<<Self as ChainFlip>::ValidatorId, <Self as ChainFlip>::Amount>
	{
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self>> + IsType<<Self as frame_system::Config>::Call>;
		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;
		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<
			Call = <Self as pallet::Config>::Call,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		/// The Ethereum Vault
		type EthereumVault: ChainVault<
			Self::RequestIndex,
			Self::PublicKey,
			Self::ValidatorId,
			RotationError<Self::ValidatorId>,
		>;
		/// The request index
		type RequestIndex: Member + Parameter + Default + Add + Copy + AtLeast32BitUnsigned;
		/// The new public key type
		type PublicKey: Member + Parameter;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current request index used in request/response (NB. this should be global and probably doesn't fit this architecture)
	#[pallet::storage]
	#[pallet::getter(fn request_idx)]
	pub(super) type RequestIdx<T: Config> = StorageValue<_, T::RequestIndex, ValueQuery>;

	/// A map acting as a list of our current vault rotations
	#[pallet::storage]
	#[pallet::getter(fn vault_rotations)]
	pub(super) type VaultRotations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RequestIndex, KeygenRequest<T::ValidatorId>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Request a key generation \[request_index, request\]
		KeygenRequestEvent(T::RequestIndex, KeygenRequest<T::ValidatorId>),
		/// Request a rotation of the vault for this chain \[request_index, request\]
		VaultRotationRequest(T::RequestIndex, VaultRotationRequest),
		/// The vault for the request has rotated \[request_index\]
		VaultRotationCompleted(T::RequestIndex),
		/// A rotation of vaults has been aborted \[request_indexes\]
		RotationAborted(Vec<T::RequestIndex>),
		/// A complete set of vaults have been rotated
		VaultsRotated,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid request idx
		InvalidRequestIdx,
		/// We have an empty validator set
		EmptyValidatorSet,
		/// The key generation response failed
		KeyResponseFailed,
		/// A vault rotation has failed
		VaultRotationCompletionFailed,
		/// A key generation response has failed
		KeygenResponseFailed,
		/// A vault rotation has failed
		VaultRotationFailed,
		/// A set of badly acting validators
		BadValidators,
		/// Failed to construct a valid chain specific payload for rotation
		FailedToConstructPayload,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// 2/3 threshold from our old validators
		#[pallet::weight(10_000)]
		pub fn witness_keygen_response(
			origin: OriginFor<T>,
			request_id: T::RequestIndex,
			response: KeygenResponse<T::ValidatorId, T::PublicKey>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T>::keygen_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn keygen_response(
			origin: OriginFor<T>,
			request_id: T::RequestIndex,
			response: KeygenResponse<T::ValidatorId, T::PublicKey>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			Self::try_is_valid(request_id)?;
			match Self::try_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(Error::<T>::from(e).into()),
			}
		}

		// 2/3 threshold from our old validators
		#[pallet::weight(10_000)]
		pub fn witness_vault_rotation_response(
			origin: OriginFor<T>,
			request_id: T::RequestIndex,
			response: VaultRotationResponse,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::<T>::vault_rotation_response(request_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn vault_rotation_response(
			origin: OriginFor<T>,
			request_id: T::RequestIndex,
			response: VaultRotationResponse,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			Self::try_is_valid(request_id)?;
			match Self::try_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(Error::<T>::from(e).into()),
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
	impl<T> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {}
	}
}

impl<T: Config> From<RotationError<T::ValidatorId>> for Error<T> {
	fn from(err: RotationError<T::ValidatorId>) -> Self {
		match err {
			RotationError::EmptyValidatorSet => Error::<T>::EmptyValidatorSet,
			RotationError::BadValidators(_) => Error::<T>::BadValidators,
			RotationError::FailedToConstructPayload => Error::<T>::FailedToConstructPayload,
			RotationError::VaultRotationCompletionFailed => {
				Error::<T>::VaultRotationCompletionFailed
			}
			RotationError::KeyResponseFailed => Error::<T>::KeyResponseFailed,
		}
	}
}

impl<T: Config> TryIndex<T::RequestIndex> for Pallet<T> {
	/// Ensure we have this index else return error
	fn try_is_valid(idx: T::RequestIndex) -> DispatchResult {
		ensure!(
			VaultRotations::<T>::contains_key(idx),
			Error::<T>::InvalidRequestIdx
		);
		Ok(())
	}
}

impl<T: Config> Index<T::RequestIndex> for Pallet<T> {
	fn next() -> T::RequestIndex {
		RequestIdx::<T>::mutate(|idx| {
			*idx = *idx + One::one();
			*idx
		})
	}

	fn invalidate(idx: T::RequestIndex) {
		VaultRotations::<T>::remove(idx);
	}

	fn is_empty() -> bool {
		VaultRotations::<T>::iter().count() == 0
	}

	fn is_valid(idx: T::RequestIndex) -> bool {
		VaultRotations::<T>::contains_key(idx)
	}
}

impl<T: Config> Pallet<T> {
	/// Register this vault rotation
	fn new_vault_rotation(index: T::RequestIndex, keygen_request: KeygenRequest<T::ValidatorId>) {
		VaultRotations::<T>::insert(index, keygen_request);
	}

	/// Abort all rotations registered and notify the `AuctionPenalty` trait of our decision to abort.
	fn abort_rotation() {
		Self::deposit_event(Event::RotationAborted(
			VaultRotations::<T>::iter().map(|(k, _)| k).collect(),
		));
		VaultRotations::<T>::remove_all();
		T::Penalty::abort();
	}
}

impl<T: Config> AuctionHandler<T::ValidatorId, T::Amount> for Pallet<T> {
	/// On completion of the Auction we would receive the proposed validators
	/// A key generation request is created for each supported chain and the process starts
	fn on_completed(winners: Vec<T::ValidatorId>, _: T::Amount) -> Result<(), AuctionError> {
		// Main entry point for the pallet
		// Create a KeyGenRequest for Ethereum
		let keygen_request = KeygenRequest {
			chain: T::EthereumVault::chain_params(),
			validator_candidates: winners.clone(),
		};

		Self::try_request(Self::next(), keygen_request).map_err(|_| AuctionError::Abort)
	}

	/// In order for the validators to be rotated we are waiting on a confirmation that the vaults
	/// have been rotated.  This is called on each block with a success acting as a confirmation
	/// that the validators can now be rotated for the new epoch.
	fn try_confirmation() -> Result<(), AuctionError> {
		// The 'exit' point for the pallet
		if Self::is_empty() {
			// We can now confirm the auction and rotate
			// The process has completed successfully
			Self::deposit_event(Event::VaultsRotated);
			Ok(())
		} else {
			// Wait on confirmation
			Err(AuctionError::NotConfirmed)
		}
	}
}

// The first phase generating the key generation requests
impl<T: Config>
	RequestResponse<
		T::RequestIndex,
		KeygenRequest<T::ValidatorId>,
		KeygenResponse<T::ValidatorId, T::PublicKey>,
		RotationError<T::ValidatorId>,
	> for Pallet<T>
{
	/// Emit as an event the key generation request, this is the first step after receiving a proposed
	/// validator set from the `AuctionEvents` trait
	fn try_request(
		index: T::RequestIndex,
		request: KeygenRequest<T::ValidatorId>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		ensure!(
			!request.validator_candidates.is_empty(),
			RotationError::EmptyValidatorSet
		);
		Self::new_vault_rotation(index,request.clone());
		Self::deposit_event(Event::KeygenRequestEvent(index, request));
		Ok(())
	}

	/// Try to process the response back for the key generation request and hand it off to the relevant
	/// chain to continue processing.  Failure would result in penalisation for the bad validators returned
	/// and the vault rotation aborted.
	fn try_response(
		index: T::RequestIndex,
		response: KeygenResponse<T::ValidatorId, T::PublicKey>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		match response {
			KeygenResponse::Success(new_public_key) => {
				// Go forth and construct
				match VaultRotations::<T>::try_get(index) {
					Ok(keygen_request) => T::EthereumVault::try_start_vault_rotation(
						index,
						new_public_key,
						keygen_request.validator_candidates.to_vec(),
					),
					Err(_) => Err(RotationError::KeyResponseFailed),
				}
			}
			KeygenResponse::Failure(bad_validators) => {
				// Abort this key generation request
				Self::abort_rotation();
				// Do as you wish with these, I wash my hands..
				T::Penalty::penalise(bad_validators);

				Err(RotationError::KeyResponseFailed)
			}
		}
	}
}

// We have now had feedback from the vault/chain that we can proceed with the final request for the
// vault rotation
impl<T: Config> ChainEvents<T::RequestIndex, T::ValidatorId, RotationError<T::ValidatorId>>
	for Pallet<T>
{
	/// Try to complete the final vault rotation with feedback from the chain implementation over
	/// the `ChainEvents` trait.  This is forwarded as a request and hence an event is emitted.
	/// Failure is handled and potential bad validators are penalised and the rotation is now aborted.
	fn try_complete_vault_rotation(
		index: T::RequestIndex,
		result: Result<VaultRotationRequest, RotationError<T::ValidatorId>>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		match result {
			// All good, forward on the request
			Ok(request) => Self::try_request(index, request),
			// Penalise if we have a set of bad validators and abort the rotation
			Err(err) => {
				if let RotationError::BadValidators(bad) = err {
					T::Penalty::penalise(bad);
				}
				Self::abort_rotation();
				Err(RotationError::VaultRotationCompletionFailed)
			}
		}
	}
}

// Request response for the vault rotation requests
impl<T: Config>
	RequestResponse<
		T::RequestIndex,
		VaultRotationRequest,
		VaultRotationResponse,
		RotationError<T::ValidatorId>,
	> for Pallet<T>
{
	/// Emit our event for the start of a vault rotation generation request.
	fn try_request(
		index: T::RequestIndex,
		request: VaultRotationRequest,
	) -> Result<(), RotationError<T::ValidatorId>> {
		Self::deposit_event(Event::VaultRotationRequest(index, request));
		Ok(())
	}

	/// Handle the response posted back on our request for a vault rotation request
	/// The request is cleared from the cache of pending requests and the relevant vault is
	/// notified
	fn try_response(
		index: T::RequestIndex,
		response: VaultRotationResponse,
	) -> Result<(), RotationError<T::ValidatorId>> {
		// Feedback to vaults
		// We have assumed here that once we have one confirmation of a vault rotation we wouldn't
		// need to rollback any if one of the group of vault rotations fails
		if let Some(keygen_request) = VaultRotations::<T>::get(index) {
			// At the moment we just have Ethereum to notify
			match keygen_request.chain {
				ChainParams::Ethereum(_) => T::EthereumVault::vault_rotated(response),
				// Leaving this to be explicit about more to come
				ChainParams::Other(_) => {}
			}
		}
		// This request is complete
		Self::invalidate(index);
		Self::deposit_event(Event::VaultRotationCompleted(index));
		Ok(())
	}
}
