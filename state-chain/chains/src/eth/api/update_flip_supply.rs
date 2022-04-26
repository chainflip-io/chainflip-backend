use crate::{eth::Tokenizable, ChainCrypto, Ethereum};
use codec::{Decode, Encode};
use ethabi::{Address, Param, ParamType, StateMutability, Token, Uint};
use frame_support::RuntimeDebug;
use sp_std::{vec, vec::Vec};

use crate::eth::{SchnorrVerificationComponents, SigData};

use super::EthereumReplayProtection;

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct UpdateFlipSupply {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The new total supply
	pub new_total_supply: Uint,
	/// The current state chain block number
	pub state_chain_block_number: Uint,
	/// The address of the stake manager - to mint or burn tokens
	pub stake_manager_address: Address,
}

impl UpdateFlipSupply {
	pub fn new_unsigned<TotalSupply: Into<Uint>, BlockNumber: Into<Uint>>(
		replay_protection: EthereumReplayProtection,
		new_total_supply: TotalSupply,
		state_chain_block_number: BlockNumber,
		stake_manager_address: &[u8; 20],
	) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(replay_protection),
			new_total_supply: new_total_supply.into(),
			state_chain_block_number: state_chain_block_number.into(),
			stake_manager_address: stake_manager_address.into(),
		};
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	pub fn signed(mut self, signature: &SchnorrVerificationComponents) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	pub fn signing_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	pub fn abi_encoded(&self) -> Vec<u8> {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::Uint(self.new_total_supply),
				Token::Uint(self.state_chain_block_number),
				Token::Address(self.stake_manager_address),
			])
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature encoded below.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}

	/// Gets the function defintion for the `updateFlipSupply` smart contract call. Loading this
	/// from the json abi definition is currently not supported in no-std, so instead we hard-code
	/// it here and verify against the abi in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi::Function::new(
			"updateFlipSupply",
			vec![
				Param::new(
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
				Param::new("newTotalSupply", ParamType::Uint(256)),
				Param::new("stateChainBlockNumber", ParamType::Uint(256)),
				Param::new("staker", ParamType::Address),
			],
			vec![],
			false,
			StateMutability::NonPayable,
		)
	}
}

#[cfg(test)]
mod test_update_flip_supply {
	use crate::eth::api::EthereumReplayProtection;

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
		const FAKE_STAKE_MANAGER_ADDRESS: [u8; 20] = asymmetrise([0xcd; 20]);
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
			&FAKE_STAKE_MANAGER_ADDRESS,
		);

		let expected_msg_hash = update_flip_supply_runtime.sig_data.msg_hash;

		assert_eq!(update_flip_supply_runtime.signing_payload(), expected_msg_hash);

		let runtime_payload = update_flip_supply_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_addr: FAKE_NONCE_TIMES_G_ADDR,
			})
			.abi_encoded();

		// Ensure signing payload isn't modified by signature.
		assert_eq!(update_flip_supply_runtime.signing_payload(), expected_msg_hash);

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
					Token::Address(FAKE_STAKE_MANAGER_ADDRESS.into()),
				])
				.unwrap()
		);
	}
}
