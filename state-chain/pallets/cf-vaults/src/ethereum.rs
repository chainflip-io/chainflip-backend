use core::convert::TryInto;

use crate::ChainParams::Ethereum;
use crate::{
	CeremonyId, ChainVault, Config, EthereumVault, Event, Pallet, RequestResponse,
	SchnorrSigTruncPubkey, ThresholdSignatureRequest, ThresholdSignatureResponse,
	VaultRotationRequestResponse, VaultRotations,
};
use cf_traits::{NonceIdentifier, NonceProvider, RotationError, VaultRotationHandler};
use ethabi::{Bytes, Function, Param, ParamType, Token};
use frame_support::pallet_prelude::*;
use sp_std::prelude::*;

pub struct EthereumChain<T: Config>(PhantomData<T>);

impl<T: Config> ChainVault for EthereumChain<T> {
	type PublicKey = T::PublicKey;
	type TransactionHash = T::TransactionHash;
	type ValidatorId = T::ValidatorId;
	type Error = RotationError<T::ValidatorId>;

	/// The initial phase has completed with success and we are notified of this from `Vaults`.
	/// Now the specifics for this chain/vault are processed.  In the case for Ethereum we request
	/// to have the function `setAggKeyWithAggKey` signed by the **old** set of validators.
	/// A payload is built and emitted as a `EthSigningTxRequest`, failing this an error is reported
	/// back to `Vaults`
	fn rotate_vault(
		ceremony_id: CeremonyId,
		new_public_key: Self::PublicKey,
		validators: Vec<Self::ValidatorId>,
	) -> Result<(), Self::Error> {
		// Create payload for signature
		match Self::encode_set_agg_key_with_agg_key(
			new_public_key.clone(),
			SchnorrSigTruncPubkey::default(),
		) {
			Ok(payload) => Self::make_request(
				ceremony_id,
				ThresholdSignatureRequest {
					validators,
					payload,
					// we want to sign with the currently active key
					public_key: EthereumVault::<T>::get().current_key,
				},
			),
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
		ThresholdSignatureRequest<T::PublicKey, T::ValidatorId>,
		ThresholdSignatureResponse<T::ValidatorId, SchnorrSigTruncPubkey>,
		RotationError<T::ValidatorId>,
	> for EthereumChain<T>
{
	/// Make the request to sign by emitting an event
	fn make_request(
		ceremony_id: CeremonyId,
		request: ThresholdSignatureRequest<T::PublicKey, T::ValidatorId>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		Pallet::<T>::deposit_event(Event::ThresholdSignatureRequest(ceremony_id, request));
		Ok(().into())
	}

	/// Try to handle the response and pass this onto `Vaults` to complete the vault rotation
	fn handle_response(
		ceremony_id: CeremonyId,
		response: ThresholdSignatureResponse<T::ValidatorId, SchnorrSigTruncPubkey>,
	) -> Result<(), RotationError<T::ValidatorId>> {
		match response {
			ThresholdSignatureResponse::Success(signature) => {
				match VaultRotations::<T>::try_get(ceremony_id) {
					Ok(vault_rotation) => {
						match Self::encode_set_agg_key_with_agg_key(
							vault_rotation
								.new_public_key
								.ok_or_else(|| RotationError::NewPublicKeyNotSet)?,
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
				T::RotationHandler::penalise(&bad_validators);
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
		let pubkey: Vec<u8> = new_public_key.into();
		// strip y-parity from key (first byte)
		let y_parity = pubkey[0];
		let x_pubkey: [u8; 32] = pubkey[1..]
			.try_into()
			.map_err(|_| ethabi::Error::InvalidData)?;
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
				Param::new(
					"newKey",
					ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(8)]),
				),
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
			Token::Tuple(vec![
				Token::Uint(x_pubkey.into()),
				Token::Uint(y_parity.into()),
			]),
		])
	}
}
