//! Types and functions that are common to ethereum.
pub mod register_claim;

use codec::{Decode, Encode};
use ethabi::{
	ethereum_types::{H256, U256},
	Address, Token, Uint,
};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::prelude::*;

//------------------------//
// TODO: these should be on-chain constants or config items. See github issue #520.
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
pub const CHAIN_ID_RINKEBY: u64 = 4;
pub const CHAIN_ID_KOVAN: u64 = 42;

pub fn stake_manager_contract_address() -> [u8; 20] {
	const ADDR: &'static str = "Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9";
	let mut buffer = [0u8; 20];
	buffer.copy_from_slice(hex::decode(ADDR).unwrap().as_slice());
	buffer
}
//--------------------------//

#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum EthereumTransactionError {
	InvalidRlp,
}

pub trait Tokenizable {
	fn tokenize(self) -> Token;
}

/// The `SigData` struct used for threshold signatures in the smart contracts.
/// See [here](https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol).
#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct SigData {
	/// The message hash aka. payload to be signed over.
	msg_hash: H256,
	/// The Schnorr signature.
	sig: Uint,
	/// The nonce value for the AggKey. Each Signature over an AggKey should have a unique nonce to prevent replay
	/// attacks.
	nonce: Uint,
	/// The address value derived from the random nonce value `k`. Also known as `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in the context of `nonceTimesGeneratorAddress`
	/// is a generated as part of each signing round (ie. as part of the Schnorr signature) to prevent certain
	/// classes of cryptographic attacks.
	k_times_g_addr: Address,
}

impl SigData {
	/// Initiate a new `SigData` with a given nonce value.
	pub fn new_empty(nonce: Uint) -> Self {
		Self {
			nonce,
			..Default::default()
		}
	}

	/// Inserts the `msg_hash` value derived from the provided calldata.
	pub fn insert_msg_hash_from(&mut self, calldata: &[u8]) {
		self.msg_hash = Keccak256::hash(calldata);
	}

	/// Add the actual signature. This method does no verification.
	pub fn insert_signature(&mut self, schnorr: &SchnorrVerificationComponents) {
		self.sig = schnorr.s.into();
		self.k_times_g_addr = schnorr.k_times_g_addr.into();
	}

	/// Get the inner signature components as a `SchnorrVerificationComponents`.
	pub fn get_signature(&self) -> SchnorrVerificationComponents {
		SchnorrVerificationComponents {
			s: self.sig.into(),
			k_times_g_addr: self.k_times_g_addr.into(),
		}
	}
}

impl Tokenizable for SigData {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Uint(self.msg_hash.0.into()),
			Token::Uint(self.sig),
			Token::Uint(self.nonce),
			Token::Address(self.k_times_g_addr),
		])
	}
}

#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SchnorrVerificationComponents {
	/// Scalar component
	pub s: [u8; 32],
	/// The challenge, expressed as a truncated keccak hash of a pair of coordinates.
	pub k_times_g_addr: [u8; 20],
}

/// Required information to construct and sign an ethereum transaction. Equivalet to [ethereum::EIP1559TransactionMessage]
/// with the following fields omitted: nonce,
///
/// The signer will need to add its account nonce and then sign and rlp-encode the transaction.
///
/// We assume the access_list (EIP-2930) is not required.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct UnsignedTransaction {
	pub chain_id: u64,
	pub max_priority_fee_per_gas: Option<U256>, // EIP-1559
	pub max_fee_per_gas: Option<U256>,
	pub gas_limit: Option<U256>,
	pub contract: Address,
	pub value: U256,
	pub data: Vec<u8>,
}

/// Raw bytes of an rlp-encoded Ethereum transaction.
pub type RawSignedTransaction = Vec<u8>;

/// Checks that the raw transaction is a valid rlp-encoded EIP1559 transaction.
pub fn verify_raw<SignerId>(
	tx: &RawSignedTransaction,
	_signer: &SignerId,
) -> Result<(), EthereumTransactionError> {
	let _decoded: ethereum::EIP1559Transaction =
		rlp::decode(&tx[..]).map_err(|_| EthereumTransactionError::InvalidRlp)?;
	// TODO check contents, signature, etc.
	Ok(())
}

/// Represents calls to Chainflip contracts requiring a threshold signature.
pub trait ChainflipContractCall {
	/// Whether or not the call has been signed.
	fn has_signature(&self) -> bool;

	/// Ethereum abi-encoded calldata for the contract call.
	fn abi_encoded(&self) -> Vec<u8>;

	/// The payload data over which the threshold signature should be made.
	fn signing_payload(&self) -> H256;

	/// Add the threshold signature to the contract call.
	fn insert_signature(&mut self, signature: &SchnorrVerificationComponents);

	/// Create a new call from the old one, with a new nonce and discarding the old signature.
	fn into_new(self, new_nonce: u64) -> Self;
}
