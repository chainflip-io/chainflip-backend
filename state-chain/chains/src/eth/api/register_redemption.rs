//! Definitions for the "registerRedemption" transaction.

use crate::{
	eth::{Ethereum, EthereumSignatureHandler, Tokenizable},
	impl_api_call_eth, ApiCall, ChainCrypto,
};

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{encode, Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{prelude::*, vec};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

/// Represents all the arguments required to build the call to StateChainGateway's
/// 'requestRedemption' function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default, MaxEncodedLen)]
pub struct RegisterRedemption {
	/// The signature handler for creating payload and inserting signature.
	pub signature_handler: EthereumSignatureHandler,
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
	pub fn new_unsigned<Amount: Into<Uint> + Clone>(
		replay_protection: EthereumReplayProtection,
		node_id: &[u8; 32],
		amount: Amount,
		address: &[u8; 20],
		expiry: u64,
		key_manager_address: Address,
		state_chain_gateway_address: Address,
		ethereum_chain_id: u64,
	) -> Self {
		Self {
			signature_handler: EthereumSignatureHandler::new_unsigned(
				replay_protection,
				Self::abi_encoded_for_payload(node_id, amount.clone().into(), address, expiry),
				key_manager_address,
				state_chain_gateway_address,
				ethereum_chain_id,
			),
			node_id: (*node_id),
			amount: amount.into(),
			address: address.into(),
			expiry: expiry.into(),
		}
	}

	/// Gets the function defintion for the `registerRedemption` smart contract call. Loading this
	/// from the json abi definition is currently not supported in no-std, so instead swe hard-code
	/// it here and verify against the abi in a unit test.
	fn get_function() -> ethabi::Function {
		ethabi_function(
			"registerRedemption",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						// sig
						ParamType::Uint(256),
						// nonce
						ParamType::Uint(256),
						// ktimesGAddr
						ParamType::Address,
					]),
				),
				ethabi_param("nodeID", ParamType::FixedBytes(32)),
				ethabi_param("amount", ParamType::Uint(256)),
				ethabi_param("funder", ParamType::Address),
				ethabi_param("expiryTime", ParamType::Uint(48)),
			],
		)
	}

	fn abi_encoded(&self) -> Vec<u8> {
		Self::get_function()
			.encode_input(&[
				self.signature_handler.sig_data.tokenize(),
				Token::FixedBytes(self.node_id.to_vec()),
				Token::Uint(self.amount),
				Token::Address(self.address),
				Token::Uint(self.expiry),
			])
			.expect(
				r#"
					This can only fail if the parameter types don't match the function signature encoded below.
					Therefore, as long as the tests pass, it can't fail at runtime.
				"#,
			)
	}

	fn abi_encoded_for_payload(
		node_id: &[u8; 32],
		amount: Uint,
		address: &[u8; 20],
		expiry: u64,
	) -> Vec<u8> {
		encode(&[
			Token::FixedBytes(Self::get_function().short_signature().to_vec()),
			Token::FixedBytes(node_id.to_vec()),
			Token::Uint(amount),
			Token::Address(address.into()),
			Token::Uint(expiry.into()),
		])
	}
}

impl_api_call_eth!(RegisterRedemption);

#[cfg(test)]
mod test_register_redemption {
	use crate::eth::{api::abi::load_abi, SchnorrVerificationComponents};

	use super::*;

	#[test]
	fn test_redemption_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const FAKE_STAKEMAN_ADDR: [u8; 20] = asymmetrise([0xdf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const EXPIRY_SECS: u64 = 10;
		const AMOUNT: u128 = 1234567890;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const TEST_ACCT: [u8; 32] = asymmetrise([0x42; 32]);
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let state_chain_gateway = load_abi("IStateChainGateway");

		let register_redemption_reference =
			state_chain_gateway.function("registerRedemption").unwrap();

		let register_redemption_runtime = RegisterRedemption::new_unsigned(
			EthereumReplayProtection { nonce: NONCE },
			&TEST_ACCT,
			AMOUNT,
			&TEST_ADDR,
			EXPIRY_SECS,
			FAKE_KEYMAN_ADDR.into(),
			FAKE_STAKEMAN_ADDR.into(),
			CHAIN_ID,
		);

		let expected_msg_hash = register_redemption_runtime.signature_handler.payload;

		assert_eq!(register_redemption_runtime.threshold_signature_payload(), expected_msg_hash);
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
					// redeemer address: address
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
