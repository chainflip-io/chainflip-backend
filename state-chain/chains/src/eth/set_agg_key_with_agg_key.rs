//! Definitions for the "registerClaim" transaction.

use super::{AggKey, ChainflipContractCall, SchnorrVerificationComponents, SigData, Tokenizable};

use codec::{Decode, Encode};
use ethabi::{ethereum_types::H256, Param, ParamType, StateMutability, Uint};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Represents all the arguments required to build the call to StakeManager's 'requestClaim' function.
#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct SetAggKeyWithAggKey {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new public key.
	pub new_key: AggKey,
}

impl ChainflipContractCall for SetAggKeyWithAggKey {
	fn has_signature(&self) -> bool {
		!self.sig_data.sig.is_zero()
	}

	fn signing_payload(&self) -> H256 {
		self.sig_data.msg_hash
	}

	fn abi_encode_with_signature(&self, signature: &SchnorrVerificationComponents) -> Vec<u8> {
		let mut call = self.clone();
		call.sig_data.insert_signature(signature);
		call.abi_encoded()
	}
}

impl SetAggKeyWithAggKey {
	pub fn new_unsigned<Nonce: Into<Uint>, Key: Into<AggKey>>(nonce: Nonce, new_key: Key) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(nonce.into()),
			new_key: new_key.into(),
		};
		calldata
			.sig_data
			.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
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

	/// Gets the function defintion for the `setAggKeyWithAggKey` smart contract call. Loading this from the json abi
	/// definition is currently not supported in no-std, so instead we hard-code it here and verify against the abi
	/// in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi::Function::new(
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
			StateMutability::NonPayable,
		)
	}
}

#[cfg(test)]
mod test_set_agg_key_with_agg_key {
	use super::*;
	use frame_support::assert_ok;
	use sp_runtime::traits::{Hash, Keccak256};

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../engine/src/eth/abis/KeyManager.json").as_ref(),
		));
	}

	#[test]
	fn test_known_payload() {
		let expected_payload = Keccak256::hash(
			hex_literal::hex!(
				"
				24969d5d 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000000 1742daac d4dbfbe6 6d4c8965
				55029587 3c683cb3 b65019d3 a53975ba
				553cc31d 00000000 00000000 00000000
				00000000 00000000 00000000 00000000
				00000001"
			)
			.as_ref(),
		);

		let call = SetAggKeyWithAggKey::new_unsigned(
			0,
			AggKey::from_pubkey_compressed(hex_literal::hex!(
				"03 1742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d"
			)),
		);

		assert_eq!(call.signing_payload(), expected_payload);
	}

	#[test]
	fn test_set_agg_key_with_agg_key_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const NONCE: u64 = 6;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_NEW_KEY_X: [u8; 32] = asymmetrise([0xcf; 32]);
		const FAKE_NEW_KEY_Y: u8 = 1;

		let key_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../engine/src/eth/abis/KeyManager.json").as_ref(),
		)
		.unwrap();

		let set_agg_key_reference = key_manager.function("setAggKeyWithAggKey").unwrap();

		let set_agg_key_runtime =
			SetAggKeyWithAggKey::new_unsigned(NONCE, (FAKE_NEW_KEY_X, FAKE_NEW_KEY_Y));

		let expected_msg_hash = set_agg_key_runtime.sig_data.msg_hash;

		assert_eq!(set_agg_key_runtime.signing_payload(), expected_msg_hash);
		let runtime_payload =
			set_agg_key_runtime.abi_encode_with_signature(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_addr: FAKE_NONCE_TIMES_G_ADDR,
			});
		// Ensure signing payload isn't modified by signature.
		assert_eq!(set_agg_key_runtime.signing_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canoncial" encoding based on the abi definition above and using the ethabi crate:
			set_agg_key_reference
				.encode_input(&vec![
					// sigData: SigData(uint, uint, uint, address)
					Token::Tuple(vec![
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
