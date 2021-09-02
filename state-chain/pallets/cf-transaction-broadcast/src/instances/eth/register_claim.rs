//! Definitions for the "registerClaim" transaction broadcast.

use super::{
	EthereumBroadcastError, SchnorrSignature, SigData, Tokenizable,
};

use cf_traits::{NonceIdentifier, NonceProvider};
use codec::{Decode, Encode};
use ethabi::{Address, FixedBytes, Param, ParamType, StateMutability, Token, Uint};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Represents all the arguments required to build the call to StakeManager's 'requestClaim' function.
#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct RegisterClaim {
	pub sig_data: SigData,
	pub node_id: FixedBytes,
	pub amount: Uint,
	pub address: Address,
	pub expiry: Uint,
}

impl RegisterClaim {
	pub fn new_unsigned<N: NonceProvider>(
		node_id: FixedBytes,
		amount: Uint,
		address: Address,
		expiry: Uint,
	) -> Result<Self, EthereumBroadcastError> {
		let mut calldata = Self {
			sig_data: SigData::new_empty(N::next_nonce(NonceIdentifier::Ethereum).into()),
			node_id,
			amount,
			address,
			expiry,
		};
		calldata.sig_data = calldata.sig_data.with_msg_hash_from(calldata.abi_encode()?.as_slice());

		Ok(calldata)
	}

	pub fn abi_encode(&self) -> Result<Vec<u8>, EthereumBroadcastError> {
		self
			.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::FixedBytes(self.node_id.clone()),
				Token::Uint(self.amount),
				Token::Address(self.address),
				Token::Uint(self.expiry),
			])
			.map_err(|_| EthereumBroadcastError::InvalidPayloadData)
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
	
	struct MockNonceProvider;
	
	const NONCE: u64 = 6;
	impl NonceProvider for MockNonceProvider {

		fn next_nonce(_: NonceIdentifier) -> cf_traits::Nonce {
			NONCE
		}
	}

	#[test]
	fn test_claim_payload() {
		// TODO: this test would be more robust with randomly generated parameters.
		use ethabi::Token;
		const EXPIRY_SECS: u64 = 10;
		const AMOUNT: u128 = 1234567890;
		const FAKE_HASH: [u8; 32] = [0x21; 32];
		const FAKE_NONCE_TIMES_G_ADDR: [u8; 20] = [0x7f; 20];
		const FAKE_SIG: [u8; 32] = [0xe1; 32];
		const TEST_ACCT: [u8; 32] = [0x42; 32];
		const TEST_ADDR: [u8; 20] = [0xcf; 20];

		let stake_manager = ethabi::Contract::load(
			std::include_bytes!("../../../../../../engine/src/eth/abis/StakeManager.json").as_ref(),
		)
		.unwrap();

		let register_claim_reference = stake_manager.function("registerClaim").unwrap();

		let mut register_claim_runtime = RegisterClaim::new_unsigned::<MockNonceProvider>(
			TEST_ACCT.into(), AMOUNT.into(), TEST_ADDR.into(), EXPIRY_SECS.into()).unwrap();

		// Erase the msg_hash.
		register_claim_runtime.sig_data.msg_hash = FAKE_HASH.into();
		register_claim_runtime.sig_data = register_claim_runtime.sig_data.with_signature(&SchnorrSignature {
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
