//! Definitions for the "registerClaim" transaction.

use crate::{
	eth::{AggKey, Ethereum, SigData, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::ParamType;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{prelude::*, vec};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

/// Represents all the arguments required to build the call to StakeManager's 'requestClaim'
/// function.
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetAggKeyWithAggKey {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new public key.
	pub new_key: AggKey,
}

impl SetAggKeyWithAggKey {
	pub fn new_unsigned<Key: Into<AggKey>>(
		replay_protection: EthereumReplayProtection,
		new_key: Key,
	) -> Self {
		let mut calldata =
			Self { sig_data: SigData::new_empty(replay_protection), new_key: new_key.into() };
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	/// Gets the function defintion for the `setAggKeyWithAggKey` smart contract call. Loading this
	/// from the json abi definition is currently not supported in no-std, so instead we hard-code
	/// it here and verify against the abi in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"setAggKeyWithAggKey",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Address,
						ParamType::Uint(256),
						ParamType::Uint(256),
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
		self.get_function()
			.encode_input(&[self.sig_data.tokenize(), self.new_key.tokenize()])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}
}

impl_api_call_eth!(SetAggKeyWithAggKey);

#[cfg(test)]
mod test_set_agg_key_with_agg_key {
	use crate::eth::SchnorrVerificationComponents;

	use super::*;
	use frame_support::assert_ok;
	use sp_runtime::traits::{Hash, Keccak256};

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
		// 000e3ae1 - function selector
		// 0000000000000000000000005fbdb2315678afecb367f032d93f642f64180aa3 - keyManager
		// 0000000000000000000000000000000000000000000000000000000000007a69 - chainId
		// 0000000000000000000000000000000000000000000000000000000000000000 - msgHash
		// 0000000000000000000000000000000000000000000000000000000000000000 - sig
		// 000000000000000000000000000000000000000000000000000000000000000f - nonce
		// 0000000000000000000000000000000000000000000000000000000000000000 - k*g
		// 1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d - newKeyX
		// 0000000000000000000000000000000000000000000000000000000000000001 - newKeyYParity
		let expected_payload = Keccak256::hash(
			hex_literal::hex!(
				"000e3ae10000000000000000000000005fbdb2315678afecb367f032d93f642f64180aa30000000000000000000000000000000000000000000000000000000000007a6900000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000f00000000000000000000000000000000000000000000000000000000000000001742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001"
			)
			.as_ref(),
		);
		let call = SetAggKeyWithAggKey::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3"),
				chain_id: 31337,
				nonce: 15,
			},
			AggKey::from_pubkey_compressed(hex_literal::hex!(
				"03 1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d"
			)),
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
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			AggKey { pub_key_x: FAKE_NEW_KEY_X, pub_key_y_parity: FAKE_NEW_KEY_Y },
		);

		let expected_msg_hash = set_agg_key_runtime.sig_data.msg_hash;

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
					// sigData: SigData(address, uint, uint, uint, uint, address)
					Token::Tuple(vec![
						Token::Address(FAKE_KEYMAN_ADDR.into()),
						Token::Uint(CHAIN_ID.into()),
						Token::Uint(expected_msg_hash.0.into()),
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
