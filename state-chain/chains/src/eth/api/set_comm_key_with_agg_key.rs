use crate::{
	eth::{Ethereum, EthereumSignatureHandler, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{encode, Address, ParamType};
use frame_support::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetCommKeyWithAggKey {
	/// The signature handler for creating payload and inserting signature.
	pub signature_handler: EthereumSignatureHandler,
	/// The new community key.
	pub new_comm_key: Address,
}

impl SetCommKeyWithAggKey {
	pub fn new_unsigned(
		replay_protection: EthereumReplayProtection,
		new_comm_key: Address,
		key_manager_address: super::eth::Address,
		ethereum_chain_id: u64,
	) -> Self {
		Self {
			signature_handler: EthereumSignatureHandler::new_unsigned(
				replay_protection,
				Self::abi_encoded_for_payload(new_comm_key),
				key_manager_address,
				key_manager_address,
				ethereum_chain_id,
			),
			new_comm_key,
		}
	}
	fn get_function() -> ethabi::Function {
		ethabi_function(
			"setCommKeyWithAggKey",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				ethabi_param("newCommKey", ParamType::Address),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		Self::get_function()
			.encode_input(&[
				self.signature_handler.sig_data.tokenize(),
				self.new_comm_key.tokenize(),
			])
			.expect(
				r#"
						This can only fail if the parameter types don't match the function signature encoded below.
						Therefore, as long as the tests pass, it can't fail at runtime.
					"#,
			)
	}

	fn abi_encoded_for_payload(new_comm_key: Address) -> Vec<u8> {
		Self::get_function()
			.short_signature()
			.into_iter()
			.chain(encode(&[new_comm_key.tokenize()]))
			.collect()
	}
}

impl_api_call_eth!(SetCommKeyWithAggKey);

#[cfg(test)]
mod test_set_comm_key_with_agg_key {

	use super::*;
	use crate::{
		eth::{tests::asymmetrise, SchnorrVerificationComponents},
		ApiCall,
	};
	use ethabi::Token;
	use ethereum_types::H160;

	use crate::eth::api::EthereumReplayProtection;

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

		let call = SetCommKeyWithAggKey::new_unsigned(
			EthereumReplayProtection { nonce: NONCE },
			H160::from(TEST_ADDR),
			FAKE_KEYMAN_ADDR.into(),
			CHAIN_ID,
		);
		let expected_msg_hash = call.signature_handler.payload;
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
				.function("setCommKeyWithAggKey")
				.unwrap()
				.encode_input(&[
					// sigData: SigData(address, uint, uint, uint, uint, address)
					Token::Tuple(vec![
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
