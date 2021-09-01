use super::{
	EthBroadcastError, RawSignedTransaction, SchnorrSignature, SigData, Tokenizable,
	UnsignedTransaction,
};
use crate::{BaseConfig, BroadcastContext};

use cf_traits::{NonceIdentifier, NonceProvider};
use codec::{Decode, Encode};
use ethabi::{Address, FixedBytes, Param, ParamType, StateMutability, Token, Uint};
use secp256k1::PublicKey;
use sp_core::H256;
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
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
	) -> Result<Self, EthBroadcastError> {
		let mut calldata = Self {
			sig_data: SigData::new_empty(N::next_nonce(NonceIdentifier::Ethereum).into()),
			node_id,
			amount,
			address,
			expiry,
		};
		calldata.sig_data.msg_hash = Keccak256::hash(calldata.abi_encode()?.as_slice());

		Ok(calldata)
	}

	pub fn abi_encode(&self) -> Result<Vec<u8>, EthBroadcastError> {
		self
			.get_function()
			.encode_input(&[
				self.sig_data.tokenize(),
				Token::FixedBytes(self.node_id.clone()),
				Token::Uint(self.amount),
				Token::Address(self.address),
				Token::Uint(self.expiry),
			])
			.map_err(|e| EthBroadcastError::InvalidPayloadData)
	}

	pub fn populate_sigdata(&mut self, sig: &SchnorrSignature) -> Result<(), EthBroadcastError> {
		let k_times_g = PublicKey::from_slice(&sig.r)
			.map(|pk| Keccak256::hash(&pk.serialize_uncompressed()))
			.map_err(|e| EthBroadcastError::InvalidSignature)?;

		self.sig_data = SigData {
			sig: sig.s.into(),
			k_times_g_addr: Address::from_slice(&k_times_g[0..20]),
			..self.sig_data
		};

		Ok(())
	}

	/// Gets the function defintion for the `registerClaim` smart contract call. Loading this from the json abi
	/// definition is currently not supported in no-std, so instead swe hard-code it here and verify against the abi
	/// in a unit test.
	pub fn get_function(&self) -> ethabi::Function {
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

// TODO: these should be on-chain constants.
const RINKEBY_CHAIN_ID: u64 = 4;
fn stake_manager_contract_address() -> Address {
	const ADDR: &'static str = "Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9";
	let mut buffer = [0u8; 20];
	buffer.copy_from_slice(hex::decode(ADDR)
		.unwrap()
		.as_slice());
	Address::from(buffer)
}

impl<T: BaseConfig> BroadcastContext<T> for RegisterClaim {
	type Payload = H256;
	type Signature = SchnorrSignature;
	type UnsignedTransaction = UnsignedTransaction;
	type SignedTransaction = RawSignedTransaction;
	type TransactionHash = H256;
	type Error = EthBroadcastError;

	fn construct_signing_payload(&self) -> Result<Self::Payload, Self::Error> {
		Ok(self.sig_data.msg_hash)
	}

	fn construct_unsigned_transaction(
		&mut self,
		sig: &Self::Signature,
	) -> Result<Self::UnsignedTransaction, Self::Error> {
		self.populate_sigdata(sig)?;
		let signed_payload = self.abi_encode()?;

		Ok(UnsignedTransaction {
			chain_id: RINKEBY_CHAIN_ID,
			max_priority_fee_per_gas: None,
			max_fee_per_gas: None,
			gas_limit: None,
			contract: stake_manager_contract_address(),
			value: 0.into(),
			data: signed_payload,
		})
	}
}
