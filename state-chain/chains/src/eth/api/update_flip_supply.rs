use crate::{
	eth::{Ethereum, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, ParamType, Token, Uint};
use frame_support::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use crate::eth::SigData;

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default)]
pub struct UpdateFlipSupply {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new total supply
	pub new_total_supply: Uint,
	/// The current state chain block number
	pub state_chain_block_number: Uint,
	/// The address of the state chain gatweay - to mint or burn tokens
	pub state_chain_gateway_address: Address,
}

impl MaxEncodedLen for UpdateFlipSupply {
	fn max_encoded_len() -> usize {
		SigData::max_encoded_len() +
			2 * <[u64; 4]>::max_encoded_len() +
			<[u8; 20]>::max_encoded_len()
	}
}

impl UpdateFlipSupply {
	pub fn new_unsigned<TotalSupply: Into<Uint>, BlockNumber: Into<Uint>>(
		replay_protection: EthereumReplayProtection,
		new_total_supply: TotalSupply,
		state_chain_block_number: BlockNumber,
		state_chain_gateway_address: &[u8; 20],
	) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(replay_protection),
			new_total_supply: new_total_supply.into(),
			state_chain_block_number: state_chain_block_number.into(),
			state_chain_gateway_address: state_chain_gateway_address.into(),
		};
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	/// Gets the function defintion for the `updateFlipSupply` smart contract call. Loading this
	/// from the json abi definition is currently not supported in no-std, so instead we hard-code
	/// it here and verify against the abi in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"updateFlipSupply",
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
				ethabi_param("newTotalSupply", ParamType::Uint(256)),
				ethabi_param("stateChainBlockNumber", ParamType::Uint(256)),
				ethabi_param("funder", ParamType::Address),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::Uint(self.new_total_supply),
				Token::Uint(self.state_chain_block_number),
				Token::Address(self.state_chain_gateway_address),
			])
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature encoded below.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}
}

impl_api_call_eth!(UpdateFlipSupply);

#[cfg(test)]
mod test_update_flip_supply {
	use crate::eth::{api::EthereumReplayProtection, SchnorrVerificationComponents};

	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/FLIP.json").as_ref(),
		));
	}

	#[test]
	fn test_update_flip_supply_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_STATE_CHAIN_GATEWAY_ADDRESS: [u8; 20] = asymmetrise([0xcd; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const NEW_TOTAL_SUPPLY: u64 = 10;
		const STATE_CHAIN_BLOCK_NUMBER: u64 = 5;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let flip_token = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/FLIP.json").as_ref(),
		)
		.unwrap();

		let flip_token_reference = flip_token.function("updateFlipSupply").unwrap();

		let update_flip_supply_runtime = UpdateFlipSupply::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			NEW_TOTAL_SUPPLY,
			STATE_CHAIN_BLOCK_NUMBER,
			&FAKE_STATE_CHAIN_GATEWAY_ADDRESS,
		);

		let expected_msg_hash = update_flip_supply_runtime.sig_data.msg_hash;

		assert_eq!(update_flip_supply_runtime.threshold_signature_payload(), expected_msg_hash);

		let runtime_payload = update_flip_supply_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded();

		// Ensure signing payload isn't modified by signature.
		assert_eq!(update_flip_supply_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canoncial" encoding based on the abi definition above and using the ethabi crate:
			flip_token_reference
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
					Token::Uint(NEW_TOTAL_SUPPLY.into()),
					Token::Uint(STATE_CHAIN_BLOCK_NUMBER.into()),
					Token::Address(FAKE_STATE_CHAIN_GATEWAY_ADDRESS.into()),
				])
				.unwrap()
		);
	}

	#[test]
	fn test_max_encoded_len() {
		cf_test_utilities::ensure_max_encoded_len_is_exact::<UpdateFlipSupply>();
	}
}
