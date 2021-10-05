use core::convert::TryInto;

use crate::ChainParams::Ethereum;
use crate::{
	ActiveChainVaultRotations, CeremonyId, ChainVault, Config, EthereumVault, Event, Pallet,
	RequestResponse, SchnorrSigTruncPubkey, ThresholdSignatureRequest, ThresholdSignatureResponse,
	VaultRotationRequestResponse,
};
use cf_traits::{RotationError, VaultRotationHandler};
use ethabi::{Bytes, Function, Param, ParamType, Token};
use frame_support::pallet_prelude::*;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
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
			[0; 32],
			new_public_key.clone(),
			SchnorrSigTruncPubkey::default(),
			// TODO: Use a separate (non ceremony_id) nonce here, will be fixed in upcoming broadcast epic
			// https://github.com/chainflip-io/chainflip-backend/pull/495
			ceremony_id,
		) {
			Ok(payload) => Self::make_request(
				ceremony_id,
				ThresholdSignatureRequest {
					validators,
					payload: Keccak256::hash(&payload).0.into(),
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
			ThresholdSignatureResponse::Success {
				message_hash,
				signature,
			} => {
				match ActiveChainVaultRotations::<T>::try_get(ceremony_id) {
					Ok(vault_rotation) => {
						// TODO: Use a separate (non ceremony_id) nonce here, will be fixed in upcoming broadcast epic
						// https://github.com/chainflip-io/chainflip-backend/pull/495
						match Self::encode_set_agg_key_with_agg_key(
							message_hash,
							vault_rotation
								.new_public_key
								.ok_or_else(|| RotationError::NewPublicKeyNotSet)?,
							signature,
							ceremony_id,
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
		message_hash: [u8; 32],
		new_public_key: T::PublicKey,
		signature: SchnorrSigTruncPubkey,
		nonce: u64,
	) -> ethabi::Result<Bytes> {
		let pubkey: Vec<u8> = new_public_key.into();
		// strip y-parity from key (first byte) and use 0 if even, 1 if odd
		// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/abstract/SchnorrSECP256K1.sol
		// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/crypto.py
		let y_parity = if pubkey[0] == 2 { 0u8 } else { 1u8 };
		let x_pubkey: [u8; 32] = pubkey[1..]
			.try_into()
			.map_err(|_| ethabi::Error::InvalidData)?;
		Function::new(
			"setAggKeyWithAggKey",
			vec![
				Param::new(
					"sigData",
					ParamType::Tuple(vec![
						// message hash
						ParamType::Uint(256),
						// sig
						ParamType::Uint(256),
						// key nonce
						ParamType::Uint(256),
						// k*G address
						ParamType::Address,
					]),
				),
				Param::new(
					"newKey",
					// pubkey_x, pubkey_y_parity
					ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(8)]),
				),
			],
			vec![],
			false,
		)
		.encode_input(&vec![
			Token::Tuple(vec![
				Token::Uint(message_hash.into()),
				Token::Uint(signature.s.into()),
				Token::Uint(nonce.into()),
				Token::Address(signature.eth_pub_key.into()),
			]),
			Token::Tuple(vec![
				Token::Uint(x_pubkey.into()),
				Token::Uint(y_parity.into()),
			]),
		])
	}
}
