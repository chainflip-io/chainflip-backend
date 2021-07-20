#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::pallet_prelude::*;
pub use pallet::*;
use crate::rotation::*;
use crate::rotation::ChainParams::Ethereum;
use ethabi::{Bytes, Function, Param, ParamType, Address, Token};
use sp_core::H160;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct EthSigningTxRequest<ValidatorId> {
	// Payload to be signed by the existing aggregate key
	pub(crate) payload: Vec<u8>,
	pub(crate) validators: Vec<ValidatorId>,
}

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
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + ChainFlip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Vaults: ConstructHandler<RequestIndex, <Self as ChainFlip>::ValidatorId, RotationError<Self::ValidatorId>> + TryIndex<RequestIndex>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		// Request this payload to be signed by the existing aggregate key
		EthSignTxRequestEvent(RequestIndex, EthSigningTxRequest<T::ValidatorId>),
	}

	#[pallet::error]
	pub enum Error<T> {
		Invalid,
		EthSigningTxResponseFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn eth_signing_tx_response(
			origin: OriginFor<T>,
			request_id: RequestIndex,
			response: EthSigningTxResponse<T::ValidatorId>
		) -> DispatchResultWithPostInfo {
			T::Vaults::try_is_valid(request_id)?;
			match Self::try_response(request_id, response) {
				Ok(_) => Ok(().into()),
				Err(_) => Err(Error::<T>::EthSigningTxResponseFailed.into())
			}
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

impl<T: Config> RequestResponse<RequestIndex, EthSigningTxRequest<T::ValidatorId>, EthSigningTxResponse<T::ValidatorId>, RotationError<T::ValidatorId>> for Pallet<T> {
	fn try_request(index: RequestIndex, request: EthSigningTxRequest<T::ValidatorId>) -> Result<(), RotationError<T::ValidatorId>> {
		// Signal to CFE to sign
		Self::deposit_event(Event::EthSignTxRequestEvent(index, request));
		Ok(().into())
	}

	fn try_response(index: RequestIndex, response: EthSigningTxResponse<T::ValidatorId>) -> Result<(), RotationError<T::ValidatorId>> {
		match response {
			EthSigningTxResponse::Success(signature) => {
				T::Vaults::try_on_completion(
					index,
					Ok(ValidatorRotationRequest::new(Ethereum(signature)))
				)
			}
			EthSigningTxResponse::Error(bad_validators) => {
				T::Vaults::try_on_completion(
					index,
					Err(ValidatorRotationError::BadValidators(bad_validators))
				)
			}
		}
	}
}

impl From<Vec<u8>> for ChainParams{
	fn from(payload: Vec<u8>) -> Self {
		Ethereum(payload)
	}
}

impl<T: Config> Construct<RequestIndex, T::ValidatorId, RotationError<T::ValidatorId>> for Pallet<T> {
	fn try_start_construction_phase(index: RequestIndex, new_public_key: NewPublicKey, validators: Vec<T::ValidatorId>) -> Result<(), RotationError<T::ValidatorId>> {
		// Create payload for signature here
		// function setAggKeyWithAggKey(SigData calldata sigData, Key calldata newKey)
		match Self::encode_set_agg_key_with_agg_key(new_public_key) {
			Ok(payload) => {
				Self::try_request(index, EthSigningTxRequest {
					validators,
					payload,
				})
			}
			Err(_) => {
				T::Vaults::try_on_completion(index, Err(ValidatorRotationError::FailedConstruct))
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	// Encode setAggKeyWithAggKey
	// This is a long approach as we are working around `no_std` limitations here for the runtime
	fn encode_set_agg_key_with_agg_key(new_public_key: NewPublicKey) -> ethabi::Result<Bytes> {
		todo!("Nonce is 0");
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
		).encode_input(&vec![
			// sigData: SigData(uint, uint, uint, address)
			Token::Tuple(vec![
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(ethabi::Uint::zero()),
				Token::Address(H160::zero()),
			]),
			// newKey: bytes32
			Token::FixedBytes(new_public_key),
		])
	}
}