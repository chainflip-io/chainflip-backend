use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default, MaxEncodedLen)]
pub struct UpdateFlipSupply {
	/// The new total supply
	pub new_total_supply: Uint,
	/// The current state chain block number
	pub state_chain_block_number: Uint,
}

impl UpdateFlipSupply {
	pub fn new<TotalSupply: Into<Uint> + Clone, BlockNumber: Into<Uint> + Clone>(
		new_total_supply: TotalSupply,
		state_chain_block_number: BlockNumber,
	) -> Self {
		Self {
			new_total_supply: new_total_supply.into(),
			state_chain_block_number: state_chain_block_number.into(),
		}
	}
}

impl EvmCall for UpdateFlipSupply {
	const FUNCTION_NAME: &'static str = "updateFlipSupply";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![("newTotalSupply", Uint::param_type()), ("stateChainBlockNumber", Uint::param_type())]
	}

	fn function_call_args(&self) -> Vec<ethabi::Token> {
		vec![self.new_total_supply.tokenize(), self.state_chain_block_number.tokenize()]
	}
}

#[cfg(test)]
mod test_update_flip_supply {
	use crate::{
		eth::api::{abi::load_abi, ApiCall, EvmReplayProtection, EvmTransactionBuilder},
		evm::SchnorrVerificationComponents,
	};

	use super::*;

	#[test]
	fn test_update_flip_supply_payload() {
		use crate::evm::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_STATE_CHAIN_GATEWAY_ADDRESS: [u8; 20] = asymmetrise([0xcd; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const NEW_TOTAL_SUPPLY: u64 = 10;
		const STATE_CHAIN_BLOCK_NUMBER: u64 = 5;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);

		let flip_token = load_abi("IStateChainGateway");

		let flip_token_reference = flip_token.function("updateFlipSupply").unwrap();

		let update_flip_supply_runtime = EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_STATE_CHAIN_GATEWAY_ADDRESS.into(),
			},
			super::UpdateFlipSupply::new(NEW_TOTAL_SUPPLY, STATE_CHAIN_BLOCK_NUMBER),
		);

		let expected_msg_hash = update_flip_supply_runtime.threshold_signature_payload();

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
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
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
