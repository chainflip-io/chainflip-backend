//! Definitions for the "registerRedemption" transaction.

use crate::{
	eth::{AggKey, Ethereum, EthereumSignatureHandler, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{encode, ParamType, Token};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{prelude::*, vec};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

/// Represents all the arguments required to build the call to StateChainGateway's
/// 'requestRedemption' function.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetAggKeyWithAggKey {
	/// The signature handler for creating payload and inserting signature.
	pub signature_handler: EthereumSignatureHandler,
	/// The new public key.
	pub new_key: AggKey,
}

impl SetAggKeyWithAggKey {
	pub fn new_unsigned<Key: Into<AggKey> + Clone>(
		replay_protection: EthereumReplayProtection,
		new_key: Key,
		key_manager_address: super::eth::Address,
		ethereum_chain_id: u64,
	) -> Self {
		Self {
			signature_handler: EthereumSignatureHandler::new_unsigned(
				replay_protection,
				Self::abi_encoded_for_payload(new_key.clone().into()),
				key_manager_address,
				key_manager_address,
				ethereum_chain_id,
			),
			new_key: new_key.into(),
		}
	}

	/// Gets the function defintion for the `setAggKeyWithAggKey` smart contract call. Loading this
	/// from the json abi definition is currently not supported in no-std, so instead we hard-code
	/// it here and verify against the abi in a unit test.
	fn get_function() -> ethabi::Function {
		ethabi_function(
			"setAggKeyWithAggKey",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				ethabi_param(
					"newKey",
					ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(8)]),
				),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		Self::get_function()
			.encode_input(&[self.signature_handler.sig_data.tokenize(), self.new_key.tokenize()])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}

	fn abi_encoded_for_payload(new_key: AggKey) -> Vec<u8> {
		encode(&[
			Token::FixedBytes(Self::get_function().short_signature().to_vec()),
			new_key.tokenize(),
		])
	}
}

impl_api_call_eth!(SetAggKeyWithAggKey);

#[cfg(test)]
mod test_set_agg_key_with_agg_key {
	use crate::eth::SchnorrVerificationComponents;

	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/KeyManager.json").as_ref(),
		));
	}

	#[test]
	fn test_known_payload() {
		let expected_payload =
			hex_literal::hex!("d45a181d77a4e10810b033734a611885d85848663b98f372f5d279309b3da0b5")
				.into();
		let call = SetAggKeyWithAggKey::new_unsigned(
			EthereumReplayProtection { nonce: 0 },
			AggKey::from_pubkey_compressed(hex_literal::hex!(
				"01 1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d"
			)),
			hex_literal::hex!("A44B9f3F5Bb8C278c1ee85D8F32517c6EFa64B0D").into(),
			31337,
		);
		assert_eq!(call.threshold_signature_payload(), expected_payload);
	}

	#[test]
	fn test_set_agg_key_with_agg_key_payload() {
		use crate::eth::{tests::asymmetrise, ParityBit};
		use ethabi::Token;
		const CHAIN_ID: u64 = 1;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const NONCE: u64 = 6;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_NEW_KEY_X: [u8; 32] = asymmetrise([0xcf; 32]);
		const FAKE_NEW_KEY_Y: ParityBit = ParityBit::Odd;

		let key_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/KeyManager.json").as_ref(),
		)
		.unwrap();

		let set_agg_key_reference = key_manager.function("setAggKeyWithAggKey").unwrap();

		let set_agg_key_runtime = SetAggKeyWithAggKey::new_unsigned(
			EthereumReplayProtection { nonce: NONCE },
			AggKey { pub_key_x: FAKE_NEW_KEY_X, pub_key_y_parity: FAKE_NEW_KEY_Y },
			FAKE_KEYMAN_ADDR.into(),
			CHAIN_ID,
		);

		let expected_msg_hash = set_agg_key_runtime.signature_handler.payload;

		assert_eq!(set_agg_key_runtime.threshold_signature_payload(), expected_msg_hash);
		let runtime_payload = set_agg_key_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded();
		// Ensure signing payload isn't modified by signature.
		assert_eq!(set_agg_key_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			set_agg_key_reference
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					// nodeId: bytes32
					Token::Tuple(vec![
						Token::Uint(FAKE_NEW_KEY_X.into()),
						Token::Uint(FAKE_NEW_KEY_Y.into()),
					]),
				])
				.unwrap()
		);
	}
}
