//! Types and functions that are common to ethereum broadcasting.
pub mod register_claim;

use crate::{BaseConfig, BroadcastContext};

use codec::{Decode, Encode};
use ethabi::{Address, Token, Uint};
use ethereum::{AccessList, TransactionAction};
use sp_core::{H256, U256};
use sp_runtime::{RuntimeDebug, traits::{Hash, Keccak256}};

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

pub enum EthereumBroadcast {
	RegisterClaim(register_claim::RegisterClaim),
}

impl<T: BaseConfig> BroadcastContext<T> for EthereumBroadcast {
	type Payload = H256;
	type Signature = SchnorrSignature;
	type UnsignedTransaction = UnsignedTransaction;
	type SignedTransaction = RawSignedTransaction;
	type TransactionHash = H256;
	type Error = EthereumBroadcastError;

	fn construct_signing_payload(&self) -> Result<Self::Payload, Self::Error> {
		match self {
			Self::RegisterClaim(rc) => Ok(rc.sig_data.msg_hash)
		}
	}

	fn construct_unsigned_transaction(
		&mut self,
		sig: &Self::Signature,
	) -> Result<Self::UnsignedTransaction, Self::Error> {
		let (contract, data) = match self {
			Self::RegisterClaim(ref mut rc) => (
				stake_manager_contract_address(),
				{
					rc.populate_sigdata(sig)?;
					rc.abi_encode()?
				}
			)
		};

		Ok(UnsignedTransaction {
			chain_id: RINKEBY_CHAIN_ID,
			max_priority_fee_per_gas: None,
			max_fee_per_gas: None,
			gas_limit: None,
			contract,
			value: U256::zero(),
			data,
		})
	}
}


#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum EthereumBroadcastError {
	InvalidPayloadData,
	InvalidSignature,
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
	pub fn new_empty(nonce: Uint) -> Self {
		Self {
			nonce,
			..Default::default()
		}
	}

	pub fn with_msg_hash_from(self, calldata: &[u8]) -> Self {
		Self {
			msg_hash:  Keccak256::hash(calldata),
			..self
		}
	}

	pub fn with_signature(self, schnorr: &SchnorrSignature) -> Self {
		Self {
			sig: schnorr.s.into(),
			k_times_g_addr: schnorr.k_times_g_addr.into(),
			..self
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

pub enum VerificationError {
	InvalidRlp(rlp::DecoderError),
}

pub fn verify(tx: RawSignedTransaction) -> Result<(), VerificationError> {
	let decoded: ethereum::EIP1559Transaction = rlp::decode(&tx[..])
		.map_err(|e| VerificationError::InvalidRlp(e))?;
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
