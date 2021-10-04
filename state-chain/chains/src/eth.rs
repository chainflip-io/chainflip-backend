//! Types and functions that are common to ethereum.
pub mod register_claim;
pub mod set_agg_key_with_agg_key;

use codec::{Decode, Encode};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint,
};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::prelude::*;
use sp_std::{
	convert::{TryFrom, TryInto},
	str,
};

//------------------------//
// TODO: these should be on-chain constants or config items. See github issue #520.
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
pub const CHAIN_ID_RINKEBY: u64 = 4;
pub const CHAIN_ID_KOVAN: u64 = 42;

pub fn stake_manager_contract_address() -> [u8; 20] {
	const ADDR: &str = "Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9";
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

/// For encoding the `Key` type as defined in https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct AggKey {
	/// The public key as a 32-byte array.
	pub pub_key_x: [u8; 32],
	/// The parity bit can be `1u8` (odd) or `0u8` (even).
	pub pub_key_y_parity: u8,
}

impl AggKey {
	/// Convert from compressed `[y, x]` coordinates where y==2 means "even" and y==3 means "odd".
	///
	/// Note that the ethereum contract expects y==0 for "even" and y==1 for "odd". We convert to the required
	/// 0 / 1 representation by subtracting 2 from the supplied values, so if the source format doesn't conform
	/// to the expected 2/3 even/odd convention, bad things will happen.
	#[cfg(feature = "std")]
	fn from_pubkey_compressed(bytes: [u8; 33]) -> Self {
		let [pub_key_y_parity, pub_key_x @ ..] = bytes;
		let pub_key_y_parity = pub_key_y_parity - 2;
		Self {
			pub_key_x,
			pub_key_y_parity,
		}
	}
}

/// [TryFrom] implementation to convert some bytes to an [AggKey].
///
/// Conversion fails *unless* the first byte is the y parity byte encoded as `2` or `3` *and* the total
/// length of the slice is 33 bytes.
impl TryFrom<&[u8]> for AggKey {
	type Error = &'static str;

	fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
		if let [pub_key_y_parity, pub_key_x @ ..] = bytes {
			if *pub_key_y_parity == 2 || *pub_key_y_parity == 3 {
				let x: [u8; 32] = pub_key_x.try_into().map_err(|e| {
					frame_support::debug::error!("Invalid aggKey format: {:?}", e);
					"Invalid aggKey format: x coordinate should be 32 bytes."
				})?;

				Ok(AggKey::from((pub_key_y_parity - 2, x)))
			} else {
				frame_support::debug::error!(
					"Invalid aggKey format: Leading byte should be 2 or 3, got {}",
					pub_key_y_parity,
				);

				Err("Invalid aggKey format: Leading byte should be 2 or 3")
			}
		} else {
			frame_support::debug::error!(
				"Invalid aggKey format: Should be 33 bytes total, got {}",
				bytes.len()
			);
			Err("Invalid aggKey format: Should be 33 bytes total.")
		}
	}
}

#[cfg(feature = "std")]
impl From<secp256k1::PublicKey> for AggKey {
	fn from(key: secp256k1::PublicKey) -> Self {
		AggKey::from_pubkey_compressed(key.serialize())
	}
}

impl From<(u8, [u8; 32])> for AggKey {
	fn from(tuple: (u8, [u8; 32])) -> Self {
		Self {
			pub_key_x: tuple.1,
			pub_key_y_parity: tuple.0,
		}
	}
}

impl From<([u8; 32], u8)> for AggKey {
	fn from(tuple: ([u8; 32], u8)) -> Self {
		Self {
			pub_key_x: tuple.0,
			pub_key_y_parity: tuple.1,
		}
	}
}

impl Tokenizable for AggKey {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Uint(Uint::from_big_endian(&self.pub_key_x[..])),
			Token::Uint(self.pub_key_y_parity.into()),
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

pub struct SchnorrSignature {
	s: libsecp256k1::curve::Scalar,
	r: libsecp256k1::PublicKey,
}

impl From<SchnorrSignature> for SchnorrVerificationComponents {
	fn from(sig: SchnorrSignature) -> Self {
		let mut k_times_g_addr = [0u8; 20];
		let k_times_g = Keccak256::hash(&sig.r.serialize()[..]);
		k_times_g_addr.copy_from_slice(&k_times_g[12..]);
		SchnorrVerificationComponents {
			s: sig.s.b32(),
			k_times_g_addr,
		}
	}
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

	/// The payload data over which the threshold signature should be made.
	fn signing_payload(&self) -> H256;

	/// Abi-encode the call with a provided signature.
	fn abi_encode_with_signature(&self, signature: &SchnorrVerificationComponents) -> Vec<u8>;
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Asymmetrisation is a very complex procedure that ensures our arrays are not symmetric.
	pub const fn asymmetrise<const T: usize>(array: [u8; T]) -> [u8; T] {
		let mut res = array;
		if T > 1 && res[0] == res[1] {
			res[0] = res[0].wrapping_add(1);
		}
		res
	}

	#[test]
	fn test_agg_key_conversion() {
		// 2 == even
		let mut bytes = [0u8; 33];
		bytes[0] = 2;
		let key = AggKey::from_pubkey_compressed(bytes);
		assert_eq!(key.pub_key_y_parity, 0);

		// 3 == odd
		let mut bytes = [0u8; 33];
		bytes[0] = 3;
		let key = AggKey::from_pubkey_compressed(bytes);
		assert_eq!(key.pub_key_y_parity, 1);
	}

	#[test]
	fn test_agg_key_conversion_with_try_from() {
		// 2 == even
		let mut bytes = vec![0u8; 33];
		bytes[0] = 2;
		let key = AggKey::try_from(&bytes[..]).expect("Should be a valid pubkey.");
		assert_eq!(key.pub_key_y_parity, 0);

		// 3 == odd
		let mut bytes = vec![0u8; 33];
		bytes[0] = 3;
		let key = AggKey::try_from(&bytes[..]).expect("Should be a valid pubkey.");
		assert_eq!(key.pub_key_y_parity, 1);
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use ethabi::ethereum_types::H160;
	use libsecp256k1::{self, PublicKey, SecretKey, curve::Affine};
	use Keccak256;

	const PUBKEY_COMPRESSED: [u8; 33] = [0u8; 33];

	fn verify_sig_data(sig_data: &SigData) -> bool {
		let mut pubkey_bytes = PUBKEY_COMPRESSED;
		pubkey_bytes[0] -= 2;
		let msg_challenge = Keccak256::hash([
			&pubkey_bytes[1..],
			&pubkey_bytes[..1],
			sig_data.msg_hash.as_bytes(), // msghash
			sig_data.k_times_g_addr.as_bytes(), // nonceTimesGeneratorAddr
		]
		.concat()
		.as_ref());

		let e = SecretKey::parse(&msg_challenge.as_fixed_bytes()).expect("msg_challenge is always a 32 byte hash");

		let s = {
			let mut buf = [0u8; 32];
			sig_data.sig.to_big_endian(&mut buf[..]);
			SecretKey::parse(&buf).unwrap()
		};

		let sG = PublicKey::from_secret_key(&s);
		
		let mut pk = PublicKey::parse_compressed(&PUBKEY_COMPRESSED).expect("public key should always be valid");
		pk.tweak_mul_assign(&e).expect("succeeds for all e != 0");

		let mut k_times_g_reconstructed: Affine = sG.into().neg();

		if sig_data.k_times_g_addr == H160::from_slice(&Keccak256::hash((sG - pk).as_bytes().as_ref())[12..]) {
			true
		} else {
			false
		}
	}

	fn test_verify_threshold_signature(
		message: &H256,
		signature: &SchnorrSignature,
		pubkey: &PublicKey,
	) -> bool {
		let mut pubkey_bytes = pubkey.serialize_compressed();
		pubkey_bytes[0] -= 2;
		let msg_challenge = Keccak256::hash([
			&pubkey_bytes[1..],
			&pubkey_bytes[..1],
			message.as_bytes(),
			signature.r.serialize().as_ref(),
		]
		.concat()
		.as_ref());

		false
	}
}
