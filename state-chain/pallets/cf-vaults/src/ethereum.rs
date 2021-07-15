#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::pallet_prelude::*;
pub use pallet::*;
use crate::rotation::*;
use crate::rotation::ChainParams::Ethereum;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct EthSigningTxRequest<ValidatorId> {
	// Payload to be signed by the existing aggregate key
	payload: Vec<u8>,
	validators: Vec<ValidatorId>,
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
		type Vaults: ConstructionManager<RequestIndex, <Self as ChainFlip>::ValidatorId> + TryIndex<RequestIndex>;
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
			Self::try_response(request_id, response)?;
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

impl<T: Config> RequestResponse<RequestIndex, EthSigningTxRequest<T::ValidatorId>, EthSigningTxResponse<T::ValidatorId>> for Pallet<T> {
	fn try_request(index: RequestIndex, request: EthSigningTxRequest<T::ValidatorId>) -> DispatchResultWithPostInfo {
		// Signal to CFE to sign
		Self::deposit_event(Event::EthSignTxRequestEvent(index, request));
		Ok(().into())
	}

	fn try_response(index: RequestIndex, response: EthSigningTxResponse<T::ValidatorId>) -> DispatchResultWithPostInfo {
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

impl<T: Config> Construct<RequestIndex, T::ValidatorId> for Pallet<T> {
	fn try_start_construction_phase(index: RequestIndex, new_public_key: NewPublicKey, validators: Vec<T::ValidatorId>) -> DispatchResultWithPostInfo {
		// Create payload for signature here
		todo!();
	}
}