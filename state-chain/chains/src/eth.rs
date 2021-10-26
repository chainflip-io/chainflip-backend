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
mod verification_tests {
	use super::*;
	use ethabi::ethereum_types::H160;
	use libsecp256k1::{
		self,
		curve::{Affine, Field, Jacobian, Scalar},
		PublicKey, SecretKey, ECMULT_CONTEXT,
	};
	use Keccak256;

	#[test]
	fn schnorr_signature_verification() {
		/*
			The below constants have been derived from integration tests with the KeyManager contract.

			In order to check if verification works, we need to use this to construct the AggKey and `SigData` as we
			normally would when submitting a function call to a threshold-signature-protected smart contract.
		*/
		const AGG_KEY_PRIV: [u8; 32] =
			hex_literal::hex!("fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba");
		const MSG_HASH: [u8; 32] =
			hex_literal::hex!("2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e");
		const SIG: [u8; 32] =
			hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");

		let (agg_key, sig_data) =
			build_test_data(AGG_KEY_PRIV, MSG_HASH.into(), SIG_NONCE, SIG.into());

		// This should pass.
		assert!(is_valid_sig(&agg_key, &sig_data));

		// Swapping the y parity bit should cause verification to fail (but without panicking!).
		let agg_key = AggKey::from((
			if agg_key.pub_key_y_parity == 0 { 1 } else { 0 },
			agg_key.pub_key_x,
		));
		assert!(!is_valid_sig(&agg_key, &sig_data));
	}

	fn build_test_data(
		// The private key component resulting from a Schnorr keygen ceremony.
		agg_key_private: [u8; 32],
		// The message has that was signed over.
		msg_hash: H256,
		// The signature nonce for this signing round, expressed as a private key.
		sig_nonce: [u8; 32],
		// The signature itself.
		sig: U256,
	) -> (AggKey, SigData) {
		let secret_key = SecretKey::parse(&agg_key_private).expect("Valid private key");
		let agg_key = AggKey::from_pubkey_compressed(
			PublicKey::from_secret_key(&secret_key).serialize_compressed(),
		);

		let k = SecretKey::parse(&sig_nonce).expect("Valid signature nonce");
		let [_, k_times_g @ ..] = PublicKey::from_secret_key(&k).serialize();
		let k_times_g_addr = H160({
			let h = Keccak256::hash(&k_times_g[..]);
			let mut res = [0u8; 20];
			res.copy_from_slice(&h.0[12..]);
			res
		});

		let sig_data = SigData {
			msg_hash,
			sig,
			nonce: Default::default(),
			k_times_g_addr,
		};

		(agg_key, sig_data)
	}

	fn is_valid_sig(agg_key: &AggKey, sig_data: &SigData) -> bool {
		// Same as in the KeyManager contract:
		//
		// uint256 msgChallenge = // "e"
		//   // solium-disable-next-line indentation
		//   uint256(keccak256(abi.encodePacked(signingPubKeyX, pubKeyYParity,
		//     msgHash, nonceTimesGeneratorAddress))
		// );
		let msg_challenge = Keccak256::hash(
			[
				&agg_key.pub_key_x[..],
				&[agg_key.pub_key_y_parity],
				sig_data.msg_hash.as_bytes(),       // msghash
				sig_data.k_times_g_addr.as_bytes(), // nonceTimesGeneratorAddr
			]
			.concat()
			.as_ref(),
		);

		// Verify: msgChallenge * signingPubKey + signature <*> generator ==
		//        nonce <*> generator
		//
		// challenge_times_pubkey + s_times_g == k_times_g
		//
		// s_times_g =? NonceTimesGenerator + challenge * PubkeyX

		// signature <*> generator
		let s_times_g = {
			let mut buf = [0u8; 32];
			sig_data.sig.to_big_endian(&mut buf[..]);
			let s = SecretKey::parse(&buf)
				.expect("Invalid signature - not a valid secp256k1 private key.");
			PublicKey::from_secret_key(&s)
		};

		// msgChallenge * signingPubKey
		let challenge_times_pubkey = {
			let public_key_point = {
				let mut point = Affine::default();
				let mut x = Field::default();
				assert!(x.set_b32(&agg_key.pub_key_x), "Invalid pubkey x coordinate");
				point.set_xo_var(&x, agg_key.pub_key_y_parity == 1);
				point
			};

			let msg_challenge_scalar = {
				let mut e = Scalar::default();
				let mut bytes = [0u8; 32];
				bytes.copy_from_slice(msg_challenge.as_ref());
				// Question: Is it ok that this prevents overflow?
				let _ = e.set_b32(&bytes);
				e
			};
			let mut res = Jacobian::default();
			ECMULT_CONTEXT.ecmult(
				&mut res,
				&Jacobian::from_ge(&public_key_point),
				&msg_challenge_scalar,
				&Scalar::default(),
			);
			res
		};

		// k_times_g_recovered ~ challenge_times_pubkey + s_times_g
		let mut k_times_g_recovered =
			Affine::from_gej(&challenge_times_pubkey.add_ge(&s_times_g.into()));
		k_times_g_recovered.x.normalize();
		k_times_g_recovered.y.normalize();

		// We now have the recovered value for k_times_g, however we only have a k_times_g_address to compare against.
		// So we need to convert our recovered k_times_g to an Ethereum address to compare against our expected value.
		let k_times_g_hash_recovered = Keccak256::hash(
			[k_times_g_recovered.x.b32(), k_times_g_recovered.y.b32()]
				.concat()
				.as_ref(),
		);

		// The signature is valid if the recovered value matches the provided one.
		k_times_g_hash_recovered[12..] == sig_data.k_times_g_addr.0
	}
}
