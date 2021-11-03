use crate::eth::Tokenizable;
use codec::{Decode, Encode};
use ethabi::{ethereum_types::H256, Param, ParamType, StateMutability, Token, Uint};
use frame_support::RuntimeDebug;
use sp_std::vec;
use sp_std::vec::Vec;

use super::{ChainflipContractCall, SchnorrVerificationComponents, SigData};

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct UpdateFlipSupply {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new total supply
	pub new_total_supply: Uint,
	/// The current state chain block number
	pub state_chain_block_number: Uint,
}

impl ChainflipContractCall for UpdateFlipSupply {
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

impl UpdateFlipSupply {
	pub fn new_unsigned<Nonce: Into<Uint>>(
		nonce: Nonce,
		new_total_supply: Uint,
		state_chain_block_number: Uint,
	) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(nonce.into()),
			new_total_supply: new_total_supply.into(),
			state_chain_block_number: state_chain_block_number.into(),
		};
		calldata
			.sig_data
			.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	fn abi_encoded(&self) -> Vec<u8> {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::Uint(self.new_total_supply),
				Token::Uint(self.state_chain_block_number),
			])
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature encoded below.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}

	/// Gets the function defintion for the `updateFlipSupply` smart contract call. Loading this from the json abi
	/// definition is currently not supported in no-std, so instead we hard-code it here and verify against the abi
	/// in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi::Function::new(
			"updateFlipSupply",
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
				Param::new("newTotalSupply", ParamType::Uint(256)),
				Param::new("stateChainBlockNumber", ParamType::Uint(256)),
			],
			vec![],
			false,
			StateMutability::NonPayable,
		)
	}
}

#[cfg(test)]
mod test_update_flip_supply {
	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		));
	}

	#[test]
	fn test_claim_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const NONCE: u64 = 6;
		const NEW_TOTAL_SUPPLY: u64 = 10;
		const STATE_CHAIN_BLOCK_NUMBER: u64 = 5;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let stake_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		)
		.unwrap();

		let stake_manager_reference = stake_manager.function("updateFlipSupply").unwrap();

		let update_flip_supply_runtime = UpdateFlipSupply::new_unsigned(
			NONCE,
			NEW_TOTAL_SUPPLY.into(),
			STATE_CHAIN_BLOCK_NUMBER.into(),
		);

		let expected_msg_hash = update_flip_supply_runtime.sig_data.msg_hash;

		assert_eq!(
			update_flip_supply_runtime.signing_payload(),
			expected_msg_hash
		);

		let runtime_payload =
			update_flip_supply_runtime.abi_encode_with_signature(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_addr: FAKE_NONCE_TIMES_G_ADDR,
			});

		// Ensure signing payload isn't modified by signature.
		assert_eq!(
			update_flip_supply_runtime.signing_payload(),
			expected_msg_hash
		);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canoncial" encoding based on the abi definition above and using the ethabi crate:
			stake_manager_reference
				.encode_input(&vec![
					// sigData: SigData(uint, uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(expected_msg_hash.0.into()),
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					Token::Uint(NEW_TOTAL_SUPPLY.into()),
					Token::Uint(STATE_CHAIN_BLOCK_NUMBER.into()),
				])
				.unwrap()
		);
	}
}
