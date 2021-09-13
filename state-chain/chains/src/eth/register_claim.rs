//! Definitions for the "registerClaim" transaction.

use super::{EthereumTransactionError, SchnorrSignature, SigData, Tokenizable};

use codec::{Decode, Encode};
use ethabi::{Address, Param, ParamType, StateMutability, Token, Uint, ethereum_types::H256};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Represents all the arguments required to build the call to StakeManager's 'requestClaim' function.
#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct RegisterClaim {
	/// The signature data for validation and replay protection.
	pub sig_data: SigData,
	/// The id (ie. Chainflip account Id) of the claimant.
	pub node_id: [u8; 32],
	/// The amount being claimed.
	pub amount: Uint,
	/// The Ethereum address to which the claim with will be withdrawn.
	pub address: Address,
	/// The expiry duration in seconds.
	pub expiry: Uint,
}

impl RegisterClaim {
	pub fn new_unsigned<Nonce: Into<Uint>, Amount: Into<Uint>>(
		nonce: Nonce,
		node_id: &[u8; 32],
		amount: Amount,
		address: &[u8; 20],
		expiry: u64,
	) -> Result<Self, EthereumTransactionError> {
		let mut calldata = Self {
			sig_data: SigData::new_empty(nonce.into()),
			node_id: (*node_id),
			amount: amount.into(),
			address: address.into(),
			expiry: expiry.into(),
		};
		calldata.sig_data = calldata
			.sig_data
			.with_msg_hash_from(calldata.abi_encode()?.as_slice());

		Ok(calldata)
	}

	pub fn get_msg_hash(&self) -> H256 {
		self.sig_data.msg_hash
	}

	pub fn abi_encode(&self) -> Result<Vec<u8>, EthereumTransactionError> {
		self.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::FixedBytes(self.node_id.to_vec()),
				Token::Uint(self.amount),
				Token::Address(self.address),
				Token::Uint(self.expiry),
			])
			.map_err(|_| EthereumTransactionError::InvalidPayloadData)
	}

	pub fn populate_sigdata(&mut self, sig: &SchnorrSignature) {
		self.sig_data = self.sig_data.with_signature(sig);
	}

	/// Gets the function defintion for the `registerClaim` smart contract call. Loading this from the json abi
	/// definition is currently not supported in no-std, so instead swe hard-code it here and verify against the abi
	/// in a unit test.
	fn get_function(&self) -> ethabi::Function {
		ethabi::Function::new(
			"registerClaim",
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
				Param::new("nodeID", ParamType::FixedBytes(32)),
				Param::new("amount", ParamType::Uint(256)),
				Param::new("staker", ParamType::Address),
				Param::new("expiryTime", ParamType::Uint(48)),
			],
			vec![],
			false,
			StateMutability::NonPayable,
		)
	}
}

#[cfg(test)]
mod test_register_claim {
	use super::*;

	#[test]
	fn test_claim_payload() {
		// TODO: this test would be more robust with randomly generated parameters.
		use ethabi::Token;
		const NONCE: u64 = 6;
		const EXPIRY_SECS: u64 = 10;
		const AMOUNT: u128 = 1234567890;
		const FAKE_HASH: [u8; 32] = [0x21; 32];
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = [0x7f; 20];
		const FAKE_SIG: [u8; 32] = [0xe1; 32];
		const TEST_ACCT: [u8; 32] = [0x42; 32];
		const TEST_ADDR: [u8; 20] = [0xcf; 20];

		let stake_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		)
		.unwrap();

		let register_claim_reference = stake_manager.function("registerClaim").unwrap();

		let mut register_claim_runtime =
			RegisterClaim::new_unsigned(NONCE, &TEST_ACCT, AMOUNT, &TEST_ADDR, EXPIRY_SECS)
				.unwrap();

		// Erase the msg_hash.
		register_claim_runtime.sig_data.msg_hash = FAKE_HASH.into();
		register_claim_runtime.sig_data =
			register_claim_runtime
				.sig_data
				.with_signature(&SchnorrSignature {
					s: FAKE_SIG,
					k_times_g_addr: FAKE_NONCE_TIMES_G_ADDR,
				});
		let runtime_payload = register_claim_runtime.abi_encode().unwrap();

		assert_eq!(
			// Our encoding:
			runtime_payload,
			// "Canoncial" encoding based on the abi definition above and using the ethabi crate:
			register_claim_reference
				.encode_input(&vec![
					// sigData: SigData(uint, uint, uint, address)
					Token::Tuple(vec![
						Token::Uint(FAKE_HASH.into()),
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
}
