use crate::{
	eth::{Ethereum, Tokenizable},
	ApiCall, ChainAbi, ChainCrypto,
};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, ParamType, Token};
use frame_support::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::eth::SigData;

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetGovKeyWithAggKey {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new gov key.
	pub new_gov_key: Address,
}

impl SetGovKeyWithAggKey {
	pub fn new_unsigned(replay_protection: EthereumReplayProtection, new_gov_key: Address) -> Self {
		let mut calldata = Self { sig_data: SigData::new_empty(replay_protection), new_gov_key };
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());
		calldata
	}
	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"setGovKeyWithAggKey",
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
				ethabi_param("newGovKey", ParamType::Address),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		self.get_function()
			.encode_input(&[self.sig_data.tokenize(), Token::Address(self.new_gov_key)])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}
}

impl ApiCall<Ethereum> for SetGovKeyWithAggKey {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	fn chain_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		self.abi_encoded()
	}

	fn is_signed(&self) -> bool {
		self.sig_data.is_signed()
	}
}

#[cfg(test)]
mod test_set_gov_key_with_agg_key {

	use super::*;
	use crate::{
		eth::{tests::asymmetrise, SchnorrVerificationComponents},
		ApiCall,
	};
	use ethabi::Token;
	use ethereum_types::H160;

	use crate::eth::api::{
		set_gov_key_with_agg_key::SetGovKeyWithAggKey, EthereumReplayProtection,
	};

	#[test]
	fn test_known_payload() {
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let key_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/KeyManager.json").as_ref(),
		)
		.unwrap();

		let call = SetGovKeyWithAggKey::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			H160::from(TEST_ADDR),
		);
		let expected_msg_hash = call.sig_data.msg_hash;
		assert_eq!(call.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			call.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded(),
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			key_manager
				.function("setGovKeyWithAggKey")
				.unwrap()
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
					Token::Address(TEST_ADDR.into()),
				])
				.unwrap()
		);
	}
}
