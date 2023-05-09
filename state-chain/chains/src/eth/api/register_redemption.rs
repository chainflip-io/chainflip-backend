//! Definitions for the "registerRedemption" transaction.

use crate::eth::{EthereumCall, Tokenizable};

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{prelude::*, vec};

/// Represents all the arguments required to build the call to StateChainGateway's
/// 'requestRedemption' function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default, MaxEncodedLen)]
pub struct RegisterRedemption {
	/// The id (ie. Chainflip account Id) of the redeemer.
	pub node_id: [u8; 32],
	/// The amount being redeemed in Flipperinos (atomic FLIP units). 1 FLIP = 10^18 Flipperinos
	pub amount: Uint,
	/// The Ethereum address to which the redemption with will be withdrawn.
	pub address: Address,
	/// The expiry duration in seconds.
	pub expiry: Uint,
}

impl RegisterRedemption {
	#[allow(clippy::too_many_arguments)]
	pub fn new<Amount: Into<Uint> + Clone>(
		node_id: &[u8; 32],
		amount: Amount,
		address: &[u8; 20],
		expiry: u64,
	) -> Self {
		Self {
			node_id: (*node_id),
			amount: amount.into(),
			address: address.into(),
			expiry: expiry.into(),
		}
	}
}

impl EthereumCall for RegisterRedemption {
	const FUNCTION_NAME: &'static str = "registerRedemption";

	fn function_params() -> Vec<(&'static str, ethabi::ParamType)> {
		vec![
			("nodeID", <[u8; 32]>::param_type()),
			("amount", Uint::param_type()),
			("funder", Address::param_type()),
			("expiryTime", ParamType::Uint(48)),
		]
	}

	fn function_call_args(&self) -> Vec<Token> {
		vec![
			self.node_id.tokenize(),
			self.amount.tokenize(),
			self.address.tokenize(),
			self.expiry.tokenize(),
		]
	}
}

#[cfg(test)]
mod test_register_redemption {
	use crate::{
		eth::{
			api::EthereumReplayProtection, EthereumTransactionBuilder,
			SchnorrVerificationComponents,
		},
		ApiCall,
	};

	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/StateChainGateway.json")
				.as_ref(),
		));
	}

	#[test]
	fn test_redemption_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_SCGW_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const EXPIRY_SECS: u64 = 10;
		const AMOUNT: u128 = 1234567890;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const TEST_ACCT: [u8; 32] = asymmetrise([0x42; 32]);
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let state_chain_gateway = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/StateChainGateway.json")
				.as_ref(),
		)
		.unwrap();

		let register_redemption_reference =
			state_chain_gateway.function("registerRedemption").unwrap();

		let register_redemption_runtime = EthereumTransactionBuilder::new_unsigned(
			EthereumReplayProtection {
				nonce: NONCE,
				chain_id: CHAIN_ID,
				key_manager_address: FAKE_KEYMAN_ADDR.into(),
				contract_address: FAKE_SCGW_ADDR.into(),
			},
			RegisterRedemption::new(&TEST_ACCT, AMOUNT, &TEST_ADDR, EXPIRY_SECS),
		);

		let expected_msg_hash = register_redemption_runtime.threshold_signature_payload();
		let runtime_payload = register_redemption_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.chain_encoded(); // Ensure signing payload isn't modified by signature.

		assert_eq!(register_redemption_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			register_redemption_reference
				.encode_input(&[
					// sigData: SigData(uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_SIG.into()),
						Token::Uint(NONCE.into()),
						Token::Address(FAKE_NONCE_TIMES_G_ADDR.into()),
					]),
					// nodeId: bytes32
					Token::FixedBytes(TEST_ACCT.into()),
					// amount: uint
					Token::Uint(AMOUNT.into()),
					// funder: address
					Token::Address(TEST_ADDR.into()),
					// epiryTime: uint48
					Token::Uint(EXPIRY_SECS.into()),
				])
				.unwrap()
		);
	}

	#[test]
	fn test_max_encoded_len() {
		cf_test_utilities::ensure_max_encoded_len_is_exact::<RegisterRedemption>();
	}
}
