//! Definitions for the "registerClaim" transaction.

use crate::{
	eth::{SigData, Tokenizable},
	ApiCall, ChainAbi, ChainCrypto, Ethereum,
};

use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::{Address, ParamType, Token, Uint};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::{prelude::*, vec};

use super::{ethabi_function, ethabi_param, EthereumReplayProtection};

/// Represents all the arguments required to build the call to StakeManager's 'requestClaim'
/// function.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct RegisterClaim {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The id (ie. Chainflip account Id) of the claimant.
	pub node_id: [u8; 32],
	/// The amount being claimed in Flipperinos (atomic FLIP units). 1 FLIP = 10^18 Flipperinos
	pub amount: Uint,
	/// The Ethereum address to which the claim with will be withdrawn.
	pub address: Address,
	/// The expiry duration in seconds.
	pub expiry: Uint,
}

impl MaxEncodedLen for RegisterClaim {
	fn max_encoded_len() -> usize {
		SigData::max_encoded_len()
		+ 2 * <[u64; 4]>::max_encoded_len() // 2 x Uint
		+ <[u8; 32]>::max_encoded_len() // 1 x [u8; 32]
		+ <[u8; 20]>::max_encoded_len() // 1 x Address
	}
}

impl RegisterClaim {
	pub fn new_unsigned<Amount: Into<Uint>>(
		replay_protection: EthereumReplayProtection,
		node_id: &[u8; 32],
		amount: Amount,
		address: &[u8; 20],
		expiry: u64,
	) -> Self {
		let mut calldata = Self {
			sig_data: SigData::new_empty(replay_protection),
			node_id: (*node_id),
			amount: amount.into(),
			address: address.into(),
			expiry: expiry.into(),
		};
		calldata.sig_data.insert_msg_hash_from(calldata.abi_encoded().as_slice());

		calldata
	}

	/// Gets the function defintion for the `registerClaim` smart contract call. Loading this from
	/// the json abi definition is currently not supported in no-std, so instead swe hard-code it
	/// here and verify against the abi in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi_function(
			"registerClaim",
			vec![
				ethabi_param(
					"sigData",
					ParamType::Tuple(vec![
						// keyManagerAddress
						ParamType::Address,
						// chainId
						ParamType::Uint(256),
						// msgHash
						ParamType::Uint(256),
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
				ethabi_param("staker", ParamType::Address),
				ethabi_param("expiryTime", ParamType::Uint(48)),
			],
		)
	}
}

impl ApiCall<Ethereum> for RegisterClaim {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		self.sig_data.msg_hash
	}

	fn signed(mut self, signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		self.sig_data.insert_signature(signature);
		self
	}

	fn abi_encoded(&self) -> <Ethereum as ChainAbi>::SignedTransaction {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
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

	fn is_signed(&self) -> bool {
		self.sig_data.is_signed()
	}
}

#[cfg(test)]
mod test_register_claim {
	use crate::eth::SchnorrVerificationComponents;

	use super::*;
	use frame_support::assert_ok;

	#[test]
	// There have been obtuse test failures due to the loading of the contract failing
	// It uses a different ethabi to the CFE, so we test separately
	fn just_load_the_contract() {
		assert_ok!(ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		));
	}

	#[test]
	fn test_claim_payload() {
		use crate::eth::tests::asymmetrise;
		use ethabi::Token;
		const FAKE_KEYMAN_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);
		const CHAIN_ID: u64 = 1;
		const NONCE: u64 = 6;
		const EXPIRY_SECS: u64 = 10;
		const AMOUNT: u128 = 1234567890;
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = asymmetrise([0x7f; 20]);
		const FAKE_SIG: [u8; 32] = asymmetrise([0xe1; 32]);
		const TEST_ACCT: [u8; 32] = asymmetrise([0x42; 32]);
		const TEST_ADDR: [u8; 20] = asymmetrise([0xcf; 20]);

		let stake_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		)
		.unwrap();

		let register_claim_reference = stake_manager.function("registerClaim").unwrap();

		let register_claim_runtime = RegisterClaim::new_unsigned(
			EthereumReplayProtection {
				key_manager_address: FAKE_KEYMAN_ADDR,
				chain_id: CHAIN_ID,
				nonce: NONCE,
			},
			&TEST_ACCT,
			AMOUNT,
			&TEST_ADDR,
			EXPIRY_SECS,
		);

		let expected_msg_hash = register_claim_runtime.sig_data.msg_hash;

		assert_eq!(register_claim_runtime.threshold_signature_payload(), expected_msg_hash);
		let runtime_payload = register_claim_runtime
			.clone()
			.signed(&SchnorrVerificationComponents {
				s: FAKE_SIG,
				k_times_g_address: FAKE_NONCE_TIMES_G_ADDR,
			})
			.abi_encoded(); // Ensure signing payload isn't modified by signature.

		assert_eq!(register_claim_runtime.threshold_signature_payload(), expected_msg_hash);

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canonical" encoding based on the abi definition above and using the ethabi crate:
			register_claim_reference
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
					// nodeId: bytes32
					Token::FixedBytes(TEST_ACCT.into()),
					// amount: uint
					Token::Uint(AMOUNT.into()),
					// staker: address
					Token::Address(TEST_ADDR.into()),
					// epiryTime: uint48
					Token::Uint(EXPIRY_SECS.into()),
				])
				.unwrap()
		);
	}

	#[test]
	fn test_max_encoded_len() {
		cf_test_utilities::ensure_max_encoded_len_is_exact::<RegisterClaim>();
	}
}
