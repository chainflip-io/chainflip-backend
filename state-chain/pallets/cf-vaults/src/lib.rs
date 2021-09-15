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
//! validator set to rotate.  The process of vault rotation is triggered by a successful auction via
//! the trait `VaultRotation::start_vault_rotation()`, which provides a list of suitable validators with which we would
//! like to proceed in rotating the vaults concerned.  The process of rotation is multi-faceted and involves a number of
//! pallets.  With the end of an epoch (by reaching a block number or forced), the `Validator` pallet requests an auction to
//! start from the `Auction` pallet.  A set of stakers are provided by the `Staking` pallet and an auction is run with the
//! outcome being shared via `VaultRotation::start_vault_rotation()`.

//! A key generation request is created for each chain supported and emitted as an event from which a ceremony is performed
//! and on success reports back with a response which is delegated to the chain specialisation which continues performing
//! steps necessary to rotate its vault implementing the `ChainVault` trait.  On completing this phase and via the trait
//! `ChainHandler`, the final step is executed with a vault rotation request being emitted.  A `VaultRotationResponse` is
//! submitted to inform whether this request to rotate has succeeded or not.
//!
//! Currently Ethereum is the only supported chain.
//!
//! During the process the network is in an auction phase, where the current validators secure the network and on successful
//! rotation of the vaults a set of nodes become validators.  Feedback on whether a rotation had occurred is provided by
//! `VaultRotation::finalize_rotation()` with which on success the validators are rotated and on failure a new auction
//! is started.
//!
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
use sp_std::prelude::*;

use cf_traits::{
	Nonce, NonceIdentifier, NonceProvider, RotationError, VaultRotation, VaultRotationHandler,
};
pub use pallet::*;
use sp_core::H160;

use crate::rotation::ChainParams::Ethereum;
use crate::rotation::*;
// we need these types exposed so subxt can use the type size
pub use crate::rotation::{KeygenRequest, VaultRotationRequest};
use ethabi::{Bytes, Function, Param, ParamType, Token};
use sp_runtime::traits::One;

#[cfg(test)]
mod mock;
pub mod rotation;
#[cfg(test)]
mod tests;

/// A signing request
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct EthSigningTxRequest<ValidatorId> {
	// Payload to be signed by the existing aggregate key
	pub(crate) payload: Vec<u8>,
	pub(crate) validators: Vec<ValidatorId>,
}

/// A response back with our signature else a list of bad validators
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum EthSigningTxResponse<ValidatorId> {
	// Signature
	Success(Vec<u8>),
	// Bad validators
	Error(Vec<ValidatorId>),
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, Nonce, NonceIdentifier};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;
		/// A public key
		type PublicKey: Member + Parameter + Into<Vec<u8>> + Default;
		/// A transaction
		type Transaction: Member + Parameter + Into<Vec<u8>> + Default;
		/// Rotation handler
		type RotationHandler: VaultRotationHandler<ValidatorId = Self::ValidatorId>;
		/// A nonce provider
		type NonceProvider: NonceProvider;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current request index used in request/response
	#[pallet::storage]
	#[pallet::getter(fn current_request)]
	pub(super) type CurrentRequest<T: Config> = StorageValue<_, RequestIndex, ValueQuery>;

	/// The Vault for this instance
	#[pallet::storage]
	#[pallet::getter(fn eth_vault)]
	pub(super) type EthereumVault<T: Config> =
		StorageValue<_, Vault<T::PublicKey, T::Transaction>, ValueQuery>;

	/// A map acting as a list of our current vault rotations
	#[pallet::storage]
	#[pallet::getter(fn vault_rotations)]
	pub(super) type VaultRotations<T: Config> =
		StorageMap<_, Blake2_128Concat, RequestIndex, KeygenRequest<T::ValidatorId>>;

	/// A map of Nonces for chains supported
	#[pallet::storage]
	#[pallet::getter(fn chain_nonces)]
	pub(super) type ChainNonces<T: Config> =
		StorageMap<_, Blake2_128Concat, NonceIdentifier, Nonce>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Request a key generation \[request_index, request\]
		KeygenRequest(RequestIndex, KeygenRequest<T::ValidatorId>),
		/// Request a rotation of the vault for this chain \[request_index, request\]
		VaultRotationRequest(RequestIndex, VaultRotationRequest),
		/// The vault for the request has rotated \[request_index\]
		VaultRotationCompleted(RequestIndex),
		/// A rotation of vaults has been aborted \[request_indexes\]
		RotationAborted(Vec<RequestIndex>),
		/// A complete set of vaults have been rotated
		VaultsRotated,
		/// Request this payload to be signed by the existing aggregate key
		EthSignTxRequest(RequestIndex, EthSigningTxRequest<T::ValidatorId>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid request index
		InvalidRequestIndex,
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
		/// Failed to sign the tx for the ethereum chain
		EthSigningTxResponseFailed,
		/// The rotation has not been confirmed
		NotConfirmed,
		/// Failed to make a key generation request
		FailedToMakeKeygenRequest,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A key generation response received from a key generation request and handled
		/// by [KeygenRequestResponse::handle_response]
		#[pallet::weight(10_000)]
		pub fn keygen_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: KeygenResponse<T::ValidatorId, T::PublicKey>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			match KeygenRequestResponse::<T>::handle_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(Error::<T>::from(e).into()),
			}
		}

		/// A vault rotation response received from a vault rotation request and handled
		/// by [VaultRotationRequestResponse::handle_response]
		#[pallet::weight(10_000)]
		pub fn vault_rotation_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: VaultRotationResponse<T::PublicKey, T::Transaction>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			match VaultRotationRequestResponse::<T>::handle_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(e) => Err(Error::<T>::from(e).into()),
			}
		}

		/// A ethereum signing transaction response received and handled
		/// by [EthereumChain::handle_response]
		#[pallet::weight(10_000)]
		pub fn eth_signing_tx_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: EthSigningTxResponse<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			match EthereumChain::<T>::handle_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(_) => Err(Error::<T>::EthSigningTxResponseFailed.into()),
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
			RotationError::InvalidRequestIndex => Error::<T>::InvalidRequestIndex,
			RotationError::NotConfirmed => Error::<T>::NotConfirmed,
			RotationError::FailedToMakeKeygenRequest => Error::<T>::FailedToMakeKeygenRequest,
		}
	}
}

impl<T: Config> NonceProvider for Pallet<T> {
	fn next_nonce(identifier: NonceIdentifier) -> Nonce {
		ChainNonces::<T>::mutate(identifier, |nonce| {
			let new_nonce = nonce.unwrap_or_default().saturating_add(One::one());
			*nonce = Some(new_nonce);
			new_nonce
		})
	}
}

impl<T: Config> Pallet<T> {
	/// Abort all rotations registered and notify the `VaultRotationHandler` trait of our decision to abort.
	fn abort_rotation() {
		Self::deposit_event(Event::RotationAborted(
			VaultRotations::<T>::iter().map(|(k, _)| k).collect(),
		));
		VaultRotations::<T>::remove_all();
		T::RotationHandler::abort();
	}

	/// Provide the next index
	fn next_index() -> RequestIndex {
		CurrentRequest::<T>::mutate(|index| {
			*index = *index + 1;
			*index
		})
	}

	fn rotations_complete() -> bool {
		VaultRotations::<T>::iter().count() == 0
	}
}

impl<T: Config> VaultRotation for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	fn start_vault_rotation(
		candidates: Vec<Self::ValidatorId>,
	) -> Result<(), RotationError<Self::ValidatorId>> {
		// Main entry point for the pallet
		ensure!(!candidates.is_empty(), RotationError::EmptyValidatorSet);
		// Create a KeyGenRequest for Ethereum
		let keygen_request = KeygenRequest {
			chain: EthereumChain::<T>::chain_params(),
			validator_candidates: candidates.clone(),
		};

		KeygenRequestResponse::<T>::make_request(Self::next_index(), keygen_request)
			.map_err(|_| RotationError::FailedToMakeKeygenRequest)
	}

	fn finalize_rotation() -> Result<(), RotationError<Self::ValidatorId>> {
		// The 'exit' point for the pallet, no rotations left to process
		if Pallet::<T>::rotations_complete() {
			// We can now confirm the auction and rotate
			// The process has completed successfully
			Self::deposit_event(Event::VaultsRotated);
			Ok(())
		} else {
			// Wait on confirmation
			Err(RotationError::NotConfirmed)
		}
	}
}

// The first phase generating the key generation requests
struct KeygenRequestResponse<T: Config>(PhantomData<T>);

impl<T: Config>
	RequestResponse<
		RequestIndex,
		KeygenRequest<T::ValidatorId>,
		KeygenResponse<T::ValidatorId, T::PublicKey>,
		RotationError<T::ValidatorId>,
	> for KeygenRequestResponse<T>
{
	/// Emit as an event the key generation request, this is the first step after receiving a proposed
	/// validator set from the `AuctionHandler::on_auction_completed()`
	fn make_request(
		index: RequestIndex,
		request: KeygenRequest<T::ValidatorId>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		VaultRotations::<T>::insert(index, request.clone());
		Pallet::<T>::deposit_event(Event::KeygenRequest(index, request));
		Ok(())
	}

	/// Try to process the response back for the key generation request and hand it off to the relevant
	/// chain to continue processing.  Failure would result in penalisation for the bad validators returned
	/// and the vault rotation aborted.
	fn handle_response(
		index: RequestIndex,
		response: KeygenResponse<T::ValidatorId, T::PublicKey>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		ensure_index!(index);
		match response {
			KeygenResponse::Success(new_public_key) => {
				// Go forth and construct
				match VaultRotations::<T>::try_get(index) {
					Ok(keygen_request) => EthereumChain::<T>::start_vault_rotation(
						index,
						new_public_key,
						keygen_request.validator_candidates.to_vec(),
					),
					Err(_) => Err(RotationError::KeyResponseFailed),
				}
			}
			KeygenResponse::Failure(bad_validators) => {
				// Abort this key generation request
				Pallet::<T>::abort_rotation();
				// Do as you wish with these, I wash my hands..
				T::RotationHandler::penalise(bad_validators);
				// Report back we have processed the failure
				Ok(().into())
			}
		}
	}
}

// We have now had feedback from the vault/chain that we can proceed with the final request for the
// vault rotation
impl<T: Config> ChainHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Error = RotationError<T::ValidatorId>;

	/// Try to complete the final vault rotation with feedback from the chain implementation over
	/// the `ChainHandler` trait.  This is forwarded as a request and hence an event is emitted.
	/// Failure is handled and potential bad validators are penalised and the rotation is now aborted.
	fn request_vault_rotation(
		index: RequestIndex,
		result: Result<VaultRotationRequest, RotationError<Self::ValidatorId>>,
	) -> Result<(), Self::Error> {
		ensure_index!(index);
		match result {
			// All good, forward on the request
			Ok(request) => VaultRotationRequestResponse::<T>::make_request(index, request),
			// Penalise if we have a set of bad validators and abort the rotation
			Err(err) => {
				if let RotationError::BadValidators(bad) = err {
					T::RotationHandler::penalise(bad);
				}
				Self::abort_rotation();
				Err(RotationError::VaultRotationCompletionFailed)
			}
		}
	}
}

// Request response for the vault rotation requests
struct VaultRotationRequestResponse<T: Config>(PhantomData<T>);
impl<T: Config>
	RequestResponse<
		RequestIndex,
		VaultRotationRequest,
		VaultRotationResponse<T::PublicKey, T::Transaction>,
		RotationError<T::ValidatorId>,
	> for VaultRotationRequestResponse<T>
{
	/// Emit our event for the start of a vault rotation generation request.
	fn make_request(
		index: RequestIndex,
		request: VaultRotationRequest,
	) -> Result<(), RotationError<T::ValidatorId>> {
		ensure_index!(index);
		Pallet::<T>::deposit_event(Event::VaultRotationRequest(index, request));
		Ok(())
	}

	/// Handle the response posted back on our request for a vault rotation request
	/// The request is cleared from the cache of pending requests and the relevant vault is
	/// notified
	fn handle_response(
		index: RequestIndex,
		response: VaultRotationResponse<T::PublicKey, T::Transaction>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		ensure_index!(index);
		// Feedback to vaults
		// We have assumed here that once we have one confirmation of a vault rotation we wouldn't
		// need to rollback any if one of the group of vault rotations fails
		match response {
			VaultRotationResponse::Success {
				old_key,
				new_key,
				tx,
			} => {
				if let Some(keygen_request) = VaultRotations::<T>::take(index) {
					// At the moment we just have Ethereum to notify
					match keygen_request.chain {
						ChainParams::Ethereum(_) => EthereumChain::<T>::vault_rotated(Vault {
							old_key,
							new_key,
							tx,
						}),
						// Leaving this to be explicit about more to come
						ChainParams::Other(_) => {}
					}
				}
				// This request is complete
				Pallet::<T>::deposit_event(Event::VaultRotationCompleted(index));
			}
			VaultRotationResponse::Failure => {
				Pallet::<T>::abort_rotation();
			}
		}

		Ok(())
	}
}

pub struct EthereumChain<T: Config>(PhantomData<T>);

impl<T: Config> ChainVault for EthereumChain<T> {
	type PublicKey = T::PublicKey;
	type Transaction = T::Transaction;
	type ValidatorId = T::ValidatorId;
	type Error = RotationError<T::ValidatorId>;

	/// Parameters required when creating key generation requests
	fn chain_params() -> ChainParams {
		ChainParams::Ethereum(vec![])
	}

	/// The initial phase has completed with success and we are notified of this from `Vaults`.
	/// Now the specifics for this chain/vault are processed.  In the case for Ethereum we request
	/// to have the function `setAggKeyWithAggKey` signed by the old set of validators.
	/// A payload is built and emitted as a `EthSigningTxRequest`, failing this an error is reported
	/// back to `Vaults`
	fn start_vault_rotation(
		index: RequestIndex,
		new_public_key: Self::PublicKey,
		validators: Vec<Self::ValidatorId>,
	) -> Result<(), Self::Error> {
		// Create payload for signature here
		// function setAggKeyWithAggKey(SigData calldata sigData, Key calldata newKey)
		match Self::encode_set_agg_key_with_agg_key(new_public_key) {
			Ok(payload) => {
				// Emit the event
				Self::make_request(
					index,
					EthSigningTxRequest {
						validators,
						payload,
					},
				)
			}
			Err(_) => {
				Pallet::<T>::abort_rotation();
				Err(RotationError::FailedToConstructPayload)
			}
		}
	}

	/// The vault for this chain has been rotated and we store this vault to storage
	fn vault_rotated(vault: Vault<Self::PublicKey, Self::Transaction>) {
		EthereumVault::<T>::set(vault);
	}
}

impl<T: Config>
	RequestResponse<
		RequestIndex,
		EthSigningTxRequest<T::ValidatorId>,
		EthSigningTxResponse<T::ValidatorId>,
		RotationError<T::ValidatorId>,
	> for EthereumChain<T>
{
	/// Make the request to sign by emitting an event
	fn make_request(
		index: RequestIndex,
		request: EthSigningTxRequest<T::ValidatorId>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		Pallet::<T>::deposit_event(Event::EthSignTxRequest(index, request));
		Ok(().into())
	}

	/// Try to handle the response and pass this onto `Vaults` to complete the vault rotation
	fn handle_response(
		index: RequestIndex,
		response: EthSigningTxResponse<T::ValidatorId>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		match response {
			EthSigningTxResponse::Success(signature) => {
				VaultRotationRequestResponse::<T>::make_request(index, Ethereum(signature).into())
			}
			EthSigningTxResponse::Error(bad_validators) => {
				T::RotationHandler::penalise(bad_validators.clone());
				Pallet::<T>::abort_rotation();
				Err(RotationError::BadValidators(bad_validators))
			}
		}
	}
}

impl<T: Config> EthereumChain<T> {
	/// Encode `setAggKeyWithAggKey` call using `ethabi`.  This is a long approach as we are working
	/// around `no_std` limitations here for the runtime.
	pub(crate) fn encode_set_agg_key_with_agg_key(
		new_public_key: T::PublicKey,
	) -> ethabi::Result<Bytes> {
		Function::new(
			"setAggKeyWithAggKey",
			vec![
				Param::new(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				Param::new("newKey", ParamType::FixedBytes(32)),
			],
			vec![],
			false,
		)
		.encode_input(&vec![
			// sigData: SigData(uint, uint, uint, address)
			Token::Tuple(vec![
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(T::NonceProvider::next_nonce(NonceIdentifier::Ethereum).into()),
				Token::Address(H160::zero()),
			]),
			// newKey: bytes32
			Token::FixedBytes(new_public_key.into()),
		])
	}
}
