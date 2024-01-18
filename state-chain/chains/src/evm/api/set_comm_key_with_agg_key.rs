use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, Token};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SetCommKeyWithAggKey {
	/// The new community key.
	pub new_comm_key: Address,
}

impl SetCommKeyWithAggKey {
	pub fn new(new_comm_key: Address) -> Self {
		Self { new_comm_key }
	}
}

impl EvmCall for SetCommKeyWithAggKey {
	const FUNCTION_NAME: &'static str = "setCommKeyWithAggKey";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("newCommKey", Address::param_type())]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![self.new_comm_key.tokenize()]
	}
}

#[cfg(test)]
mod test_set_comm_key_with_agg_key {
	use super::*;
	use crate::{
		eth::api::abi::load_abi,
		evm::{api::EvmTransactionBuilder, tests::asymmetrise, SchnorrVerificationComponents},
		ApiCall,
	};

	use crate::evm::api::EvmReplayProtection;

	#[test]
	fn test_known_payload() {
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let key_manager = load_abi("IKeyManager");

		let tx_builder = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_KEYMAN_ADDR.into(),
			},
			super::SetCommKeyWithAggKey::new(Address::from(TEST_ADDR)),
		);

		assert_eq!(
			// Our encoding:
			tx_builder
				.signed(&SchnorrVerificationComponents {
					s: FAKE_SIG,
					k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
				})
				.chain_encoded(),
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			key_manager
				.function("setCommKeyWithAggKey")
				.unwrap()
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
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
