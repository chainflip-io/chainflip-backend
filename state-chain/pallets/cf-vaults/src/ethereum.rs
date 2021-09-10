use crate::ChainParams::Ethereum;
use crate::{
	CeremonyId, ChainVault, Config, EthereumVault, Event, Pallet, RequestResponse,
	SchnorrSigTruncPubkey, ThresholdSignatureRequest, ThresholdSignatureResponse,
	VaultRotationRequestResponse, VaultRotations,
};
use cf_traits::{Chainflip, NonceIdentifier, NonceProvider, RotationError, VaultRotationHandler};
use ethabi::{Bytes, Function, Param, ParamType, Token};
use frame_support::pallet_prelude::*;
use sp_std::prelude::*;

pub struct EthereumChain<T: Config>(PhantomData<T>);

impl<T: Config> ChainVault for EthereumChain<T> {
	type PublicKey = T::PublicKey;
	type TransactionHash = T::TransactionHash;
	type AccountId = <T as Chainflip>::AccountId;
	type Error = RotationError<<T as Chainflip>::AccountId>;

	/// The initial phase has completed with success and we are notified of this from `Vaults`.
	/// Now the specifics for this chain/vault are processed.  In the case for Ethereum we request
	/// to have the function `setAggKeyWithAggKey` signed by the **old** set of validators.
	/// A payload is built and emitted as a `EthSigningTxRequest`, failing this an error is reported
	/// back to `Vaults`
	fn rotate_vault(
		ceremony_id: CeremonyId,
		new_public_key: Self::PublicKey,
		validators: Vec<Self::AccountId>,
	) -> Result<(), Self::Error> {
		// Create payload for signature
		match Self::encode_set_agg_key_with_agg_key(
			new_public_key.clone(),
			SchnorrSigTruncPubkey::default(),
		) {
			Ok(payload) => {
				// Emit the event
				Self::make_request(
					ceremony_id,
					ThresholdSignatureRequest {
						validators,
						payload,
						public_key: EthereumVault::<T>::get().previous_key,
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
	fn vault_rotated(new_public_key: Self::PublicKey, tx_hash: Self::TransactionHash) {
		EthereumVault::<T>::mutate(|vault| {
			(*vault).previous_key = (*vault).current_key.clone();
			(*vault).current_key = new_public_key;
			(*vault).tx_hash = tx_hash;
		});
	}
}

impl<T: Config>
	RequestResponse<
		CeremonyId,
		ThresholdSignatureRequest<T::PublicKey, <T as Chainflip>::AccountId>,
		ThresholdSignatureResponse<<T as Chainflip>::AccountId, SchnorrSigTruncPubkey>,
		RotationError<<T as Chainflip>::AccountId>,
	> for EthereumChain<T>
{
	/// Make the request to sign by emitting an event
	fn make_request(
		ceremony_id: CeremonyId,
		request: ThresholdSignatureRequest<T::PublicKey, <T as Chainflip>::AccountId>,
	) -> Result<(), RotationError<<T as Chainflip>::AccountId>> {
		Pallet::<T>::deposit_event(Event::ThresholdSignatureRequest(ceremony_id, request));
		Ok(().into())
	}

	/// Try to handle the response and pass this onto `Vaults` to complete the vault rotation
	fn handle_response(
		ceremony_id: CeremonyId,
		response: ThresholdSignatureResponse<<T as Chainflip>::AccountId, SchnorrSigTruncPubkey>,
	) -> Result<(), RotationError<<T as Chainflip>::AccountId>> {
		match response {
			ThresholdSignatureResponse::Success(signature) => {
				match VaultRotations::<T>::try_get(ceremony_id) {
					Ok(vault_rotation) => {
						match Self::encode_set_agg_key_with_agg_key(
							vault_rotation.new_public_key,
							signature,
						) {
							Ok(payload) => {
								// Emit the event
								VaultRotationRequestResponse::<T>::make_request(
									ceremony_id,
									Ethereum(payload).into(),
								)
							}
							Err(_) => {
								Pallet::<T>::abort_rotation();
								Err(RotationError::FailedToConstructPayload)
							}
						}
					}
					Err(_) => Err(RotationError::InvalidCeremonyId),
				}
			}
			ThresholdSignatureResponse::Error(bad_validators) => {
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
		signature: SchnorrSigTruncPubkey,
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
			Token::Tuple(vec![
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(signature.s.into()),
				Token::Uint(T::NonceProvider::next_nonce(NonceIdentifier::Ethereum).into()),
				Token::Address(signature.eth_pub_key.into()),
			]),
			// newKey: bytes32
			Token::FixedBytes(new_public_key.into()),
		])
	}
}
