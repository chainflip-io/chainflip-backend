//! Types and functions that are common to ethereum.
pub mod register_claim;

use codec::{Decode, Encode};
use ethabi::{
	ethereum_types::{H256, U256},
	Address, Token, Uint,
};
use ethereum::{AccessList, TransactionAction};
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::prelude::*;

//------------------------//
// TODO: these should be on-chain constants or config items.
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
	InvalidPayloadData,
	InvalidSignature,
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
	/// The public key derived from the random nonce value `k`. Also known as `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in this context is a generated as part of each signing
	/// round (ie. as part of the Schnorr signature) to prevent certain classes of cryptographic attacks.
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

	/// Returns a copy of the SigData with the `msg_hash` value derived from the provided calldata.
	pub fn with_msg_hash_from(self, calldata: &[u8]) -> Self {
		Self {
			msg_hash: Keccak256::hash(calldata),
			..self
		}
	}

	/// Add the actual signature. This method does no verification.
	pub fn with_signature(self, schnorr: &SchnorrSignature) -> Self {
		Self {
			sig: schnorr.s.into(),
			k_times_g_addr: schnorr.k_times_g_addr.into(),
			..self
		}
	}

	/// Get the inner signature components as a `SchnorrSignature`.
	pub fn get_signature(&self) -> SchnorrSignature {
		SchnorrSignature {
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
pub struct SchnorrSignature {
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

pub type RawSignedTransaction = Vec<u8>;

pub fn verify_raw<SignerId>(
	tx: &RawSignedTransaction,
	_signer: &SignerId,
) -> Result<(), EthereumTransactionError> {
	let decoded: ethereum::EIP1559Transaction =
		rlp::decode(&tx[..]).map_err(|_| EthereumTransactionError::InvalidRlp)?;
	// TODO check contents, signature, etc.
	Ok(())
}

pub struct SignedTransaction {
	pub chain_id: u64, // Constant

	// Determined by the signer
	pub nonce: U256,
	pub max_priority_fee_per_gas: U256, // EIP-1559
	pub max_fee_per_gas: U256,
	pub gas_limit: U256,

	pub action: TransactionAction, // always `Call(contract_address)`
	pub value: U256,               // Always 0 (?)
	pub data: Vec<u8>,             // The abi-encoded contract call.

	// EIP-2930, assume for now that this will remain empty.
	pub access_list: AccessList,

	// Signature data
	/// The V field of the signature; the LS bit described which half of the curve our point falls
	/// in. It can be 0 or 1.
	pub odd_y_parity: bool,
	/// The R field of the signature; helps describe the point on the curve.
	pub r: H256,
	/// The S field of the signature; helps describe the point on the curve.
	pub s: H256,
}
