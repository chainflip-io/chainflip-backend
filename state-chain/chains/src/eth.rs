//! Types and functions that are common to ethereum.
pub mod register_claim;
pub mod set_agg_key_with_agg_key;

use codec::{Decode, Encode};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint,
};
use libsecp256k1::{
	curve::{Affine, Field, Jacobian, Scalar},
	PublicKey, SecretKey, ECMULT_CONTEXT,
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
	InvalidSignature,
	InvalidRecoveryId,
	WrongChainId,
	WrongGasLimit,
	WrongData,
	WrongValue,
	WrongContractAddress,
	WrongAction,
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
		self.msg_hash = H256(Keccak256::hash(calldata).0);
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

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AggKeyVerificationError {
	InvalidSignature,
	InvalidPubkey,
	NoMatch,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum ParityBit {
	Odd,
	Even,
}

impl ParityBit {
	pub fn is_odd(&self) -> bool {
		match self {
			Self::Odd => true,
			_ => false,
		}
	}

	pub fn is_even(&self) -> bool {
		match self {
			Self::Even => true,
			_ => false,
		}
	}
}

/// Ethereum contracts use `0` and `1` to represent parity bits.
impl From<ParityBit> for Uint {
	fn from(parity_bit: ParityBit) -> Self {
		match parity_bit {
			ParityBit::Odd => Uint::one(),
			ParityBit::Even => Uint::zero(),
		}
	}
}

/// For encoding the `Key` type as defined in https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct AggKey {
	/// The public key as a 32-byte array.
	pub pub_key_x: [u8; 32],
	/// The parity bit can be odd or even.
	pub pub_key_y_parity: ParityBit,
}

impl AggKey {
	/// Convert from compressed `[y, x]` coordinates where y==2 means "even" and y==3 means "odd".
	///
	/// Note that the ethereum contract expects y==0 for "even" and y==1 for "odd". We convert to the required
	/// 0 / 1 representation by subtracting 2 from the supplied values, so if the source format doesn't conform
	/// to the expected 2/3 even/odd convention, bad things will happen.
	pub fn from_pubkey_compressed(bytes: [u8; 33]) -> Self {
		let [pub_key_y_parity, pub_key_x @ ..] = bytes;
		let pub_key_y_parity = if pub_key_y_parity == 2 {
			ParityBit::Even
		} else {
			ParityBit::Odd
		};
		Self {
			pub_key_x,
			pub_key_y_parity,
		}
	}

	/// Create a public `AggKey` from the private key component.
	pub fn from_private_key_bytes(agg_key_private: [u8; 32]) -> Self {
		let secret_key = SecretKey::parse(&agg_key_private).expect("Valid private key");
		AggKey::from_pubkey_compressed(
			PublicKey::from_secret_key(&secret_key).serialize_compressed(),
		)
	}

	/// Convert to 'compressed pubkey` format where a leading `2` means 'even parity bit' and a leading `3` means 'odd'.
	pub fn to_pubkey_compressed(&self) -> [u8; 33] {
		let mut result = [0u8; 33];
		result[0] = match self.pub_key_y_parity {
			ParityBit::Odd => 3u8,
			ParityBit::Even => 2u8,
		};
		result[1..].copy_from_slice(&self.pub_key_x[..]);
		result
	}

	/// Compute the message challenge e according to the format expected by the ethereum contracts.
	///
	/// From the [Schnorr verification contract]
	/// (https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/abstract/SchnorrSECP256K1.sol):
	///
	/// ```python
	/// uint256 msgChallenge = // "e"
	///   uint256(keccak256(abi.encodePacked(signingPubKeyX, pubKeyYParity,
	///     msgHash, nonceTimesGeneratorAddress))
	/// );
	/// ```
	pub fn message_challenge(&self, msg_hash: &[u8; 32], k_times_g_addr: &[u8; 20]) -> [u8; 32] {
		// Note the contract expects a packed u8 of 0 (even) or 1 (odd).
		let parity_bit_uint_packed = match self.pub_key_y_parity {
			ParityBit::Odd => 1u8,
			ParityBit::Even => 0u8,
		};
		Keccak256::hash(
			[
				&self.pub_key_x[..],
				&[parity_bit_uint_packed],
				&msg_hash[..],
				&k_times_g_addr[..],
			]
			.concat()
			.as_ref(),
		)
		.into()
	}

	/// Verify a signature against a given message hash for this public key.
	pub fn verify(
		&self,
		msg_hash: &[u8; 32],
		sig: &SchnorrVerificationComponents,
	) -> Result<(), AggKeyVerificationError> {
		//----
		// Verification:
		//     msgChallenge * signingPubKey + signature * generator == nonce * generator
		// We don't have nonce, we have k_times_g_addr so will instead verify like this:
		//     encode_addr(msgChallenge * signingPubKey + signature * generator) == encode_addr(nonce * generator)
		// Simplified:
		//     encode_addr(msgChallenge * signingPubKey + signature * generator) == k_times_g_addr
		//----

		// signature * generator
		let s_times_g = {
			let s =
				SecretKey::parse(&sig.s).map_err(|_| AggKeyVerificationError::InvalidSignature)?;
			PublicKey::from_secret_key(&s)
		};

		// msgChallenge * signingPubKey
		let challenge_times_pubkey = {
			// Derive the public key point equivalent from the AggKey: effectively the inverse of
			// AggKey::from_pubkey_compressed();
			let public_key_point = {
				let mut point = Affine::default();
				let mut x = Field::default();
				if !x.set_b32(&self.pub_key_x) {
					return Err(AggKeyVerificationError::InvalidPubkey);
				}
				point.set_xo_var(&x, self.pub_key_y_parity.is_odd());
				point
			};

			// Convert the message challenge to a Scalar value so it can be multiplied with the point.
			let msg_challenge = self.message_challenge(msg_hash, &sig.k_times_g_addr);
			let msg_challenge_scalar = {
				let mut e = Scalar::default();
				let mut bytes = [0u8; 32];
				bytes.copy_from_slice(msg_challenge.as_ref());
				// Question: Is it ok that this prevents overflow?
				let _ = e.set_b32(&bytes);
				e
			};

			// Some mathematical magic - multiplies the point and scalar value.
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
		if k_times_g_hash_recovered[12..] == sig.k_times_g_addr {
			Ok(())
		} else {
			Err(AggKeyVerificationError::NoMatch)
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

/// [TryFrom] implementation to convert some bytes to an [AggKey].
///
/// Conversion fails *unless* the first byte is the y parity byte encoded as `2` or `3` *and* the total
/// length of the slice is 33 bytes.
impl TryFrom<&[u8]> for AggKey {
	type Error = &'static str;

	fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
		if bytes.len() != 33 {
			frame_support::debug::error!(
				"Invalid aggKey format: Should be 33 bytes total, got {}",
				bytes.len()
			);
			return Err("Invalid aggKey format: Should be 33 bytes total.");
		}

		let pub_key_y_parity = match bytes[0] {
			2 => Ok(ParityBit::Even),
			3 => Ok(ParityBit::Odd),
			invalid => {
				frame_support::debug::error!(
					"Invalid aggKey format: Leading byte should be 2 or 3, got {}",
					invalid,
				);

				Err("Invalid aggKey format: Leading byte should be 2 or 3")
			}
		}?;

		let pub_key_x: [u8; 32] = bytes[1..].try_into().map_err(|e| {
			frame_support::debug::error!("Invalid aggKey format: {:?}", e);
			"Invalid aggKey format: x coordinate should be 32 bytes."
		})?;

		Ok(Self {
			pub_key_x,
			pub_key_y_parity,
		})
	}
}

impl From<AggKey> for Vec<u8> {
	fn from(agg_key: AggKey) -> Self {
		agg_key.to_pubkey_compressed().to_vec()
	}
}

impl TryFrom<Vec<u8>> for AggKey {
	type Error = &'static str;

	fn try_from(serialized: Vec<u8>) -> Result<Self, Self::Error> {
		serialized.as_slice().try_into()
	}
}

#[cfg(feature = "std")]
impl From<secp256k1::PublicKey> for AggKey {
	fn from(key: secp256k1::PublicKey) -> Self {
		AggKey::from_pubkey_compressed(key.serialize())
	}
}

#[derive(Encode, Decode, Copy, Clone, RuntimeDebug, PartialEq, Eq, Default)]
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

impl UnsignedTransaction {
	/// Returns an error if any of the following items don't match the recovered message:
	///
	/// - chain_id
	/// - gas_limit
	/// - data
	/// - value
	/// - contract
	///
	/// OR if the the wrong `action` was not a call to the correct contract.
	///
	/// See [EthereumTransactionError].
	pub fn match_against_recovered(
		&self,
		recovered: &ethereum::EIP1559TransactionMessage,
	) -> Result<(), EthereumTransactionError> {
		if self.chain_id != recovered.chain_id {
			return Err(EthereumTransactionError::WrongChainId);
		}

		if let Some(x) = self.gas_limit {
			if x.0 != recovered.gas_limit.0 {
				return Err(EthereumTransactionError::WrongGasLimit);
			}
		}

		if self.data != recovered.input {
			return Err(EthereumTransactionError::WrongData);
		}

		if self.value.0 != recovered.value.0 {
			return Err(EthereumTransactionError::WrongValue);
		}

		match recovered.action {
			ethereum::TransactionAction::Call(address) => {
				if address.as_bytes() != self.contract.as_bytes() {
					return Err(EthereumTransactionError::WrongContractAddress);
				}
			}
			ethereum::TransactionAction::Create => {
				return Err(EthereumTransactionError::WrongAction)
			}
		}

		Ok(())
	}
}

/// Raw bytes of an rlp-encoded Ethereum transaction.
pub type RawSignedTransaction = Vec<u8>;

/// Checks that the raw transaction is a valid rlp-encoded EIP1559 transaction.
///
/// See [here](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-1559.md) for details.
pub fn verify_transaction(
	unsigned: &UnsignedTransaction,
	signed: &RawSignedTransaction,
	address: &Address,
) -> Result<(), EthereumTransactionError> {
	let decoded_tx: ethereum::EIP1559Transaction =
		rlp::decode(&signed[..]).map_err(|_| EthereumTransactionError::InvalidRlp)?;

	let message: ethereum::EIP1559TransactionMessage = decoded_tx.clone().into();

	let public_key = libsecp256k1::recover(
		&libsecp256k1::Message::parse(message.hash().as_fixed_bytes()),
		&libsecp256k1::Signature::parse_standard_slice(
			[decoded_tx.r.as_bytes(), decoded_tx.s.as_bytes()]
				.concat()
				.as_slice(),
		)
		.map_err(|_| EthereumTransactionError::InvalidSignature)?,
		&libsecp256k1::RecoveryId::parse_rpc(if decoded_tx.odd_y_parity { 27 } else { 28 })
			.map_err(|_| EthereumTransactionError::InvalidRecoveryId)?,
	)
	.map_err(|_| EthereumTransactionError::InvalidSignature)?;

	let expected_address = &Keccak256::hash(&public_key.serialize()[1..])[12..];

	if expected_address != address.as_bytes() {
		return Err(EthereumTransactionError::InvalidSignature);
	}

	unsigned.match_against_recovered(&message)
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
		assert!(key.pub_key_y_parity.is_even());

		// 3 == odd
		let mut bytes = [0u8; 33];
		bytes[0] = 3;
		let key = AggKey::from_pubkey_compressed(bytes);
		assert!(key.pub_key_y_parity.is_odd());
	}

	#[test]
	fn test_agg_key_conversion_with_try_from() {
		// 2 == even
		let mut bytes = vec![0u8; 33];
		bytes[0] = 2;
		let key = AggKey::try_from(&bytes[..]).expect("Should be a valid pubkey.");
		assert!(key.pub_key_y_parity.is_even());

		// 3 == odd
		let mut bytes = vec![0u8; 33];
		bytes[0] = 3;
		let key = AggKey::try_from(&bytes[..]).expect("Should be a valid pubkey.");
		assert!(key.pub_key_y_parity.is_odd());
	}
}

#[cfg(test)]
mod verification_tests {
	use super::*;
	use ethereum::{EIP1559Transaction, EIP1559TransactionMessage};
	use frame_support::{assert_err, assert_ok};
	use libsecp256k1::{PublicKey, SecretKey};
	use Keccak256;

	#[test]
	fn test_schnorr_signature_verification() {
		/*
			The below constants have been derived from integration tests with the KeyManager contract.

			In order to check if verification works, we need to use this to construct the AggKey and `SigData` as we
			normally would when submitting a function call to a threshold-signature-protected smart contract.
		*/
		const AGG_KEY_PRIV: [u8; 32] =
			hex_literal::hex!("fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba");
		const AGG_KEY_PUB: [u8; 33] =
			hex_literal::hex!("0331b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae");
		const MSG_HASH: [u8; 32] =
			hex_literal::hex!("2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e");
		const SIG: [u8; 32] =
			hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
		const SIG_NONCE: [u8; 32] =
			hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");

		let agg_key = AggKey::from_private_key_bytes(AGG_KEY_PRIV);
		assert_eq!(agg_key.to_pubkey_compressed(), AGG_KEY_PUB);

		let k = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let [_, k_times_g @ ..] = PublicKey::from_secret_key(&k).serialize();
		let k_times_g_addr = {
			let h = Keccak256::hash(&k_times_g[..]);
			let mut res = [0u8; 20];
			res.copy_from_slice(&h.0[12..]);
			res
		};

		let sig = SchnorrVerificationComponents {
			s: SIG,
			k_times_g_addr,
		};

		// This should pass.
		assert_ok!(agg_key.verify(&MSG_HASH, &sig));

		// Swapping the y parity bit should cause verification to fail.
		let bad_agg_key = AggKey {
			pub_key_y_parity: if agg_key.pub_key_y_parity.is_even() {
				ParityBit::Odd
			} else {
				ParityBit::Even
			},
			pub_key_x: agg_key.pub_key_x,
		};
		assert_err!(
			bad_agg_key.verify(&MSG_HASH, &sig),
			AggKeyVerificationError::NoMatch
		);

		// Providing the wrong signature should fail.
		assert!(agg_key
			.verify(
				&MSG_HASH,
				&SchnorrVerificationComponents {
					s: SIG.map(|i| i + 1),
					k_times_g_addr
				}
			)
			.is_err(),);

		// Providing the wrong nonce should fail.
		assert_err!(
			agg_key.verify(
				&MSG_HASH,
				&SchnorrVerificationComponents {
					s: SIG,
					k_times_g_addr: k_times_g_addr.map(|i| i + 1),
				}
			),
			AggKeyVerificationError::NoMatch
		);
	}

	#[test]
	fn test_ethereum_signature_verification() {
		let unsigned = UnsignedTransaction {
			chain_id: 42,
			max_fee_per_gas: U256::from(1_000_000_000u32).into(),
			gas_limit: U256::from(21_000u32).into(),
			contract: [0xcf; 20].into(),
			value: 0.into(),
			data: b"do_something()".to_vec(),
			..Default::default()
		};

		let msg = EIP1559TransactionMessage {
			chain_id: unsigned.chain_id,
			nonce: 0.into(),
			max_priority_fee_per_gas: 0.into(),
			max_fee_per_gas: unsigned.max_fee_per_gas.unwrap().into(),
			gas_limit: unsigned.gas_limit.unwrap(),
			action: ethereum::TransactionAction::Call(unsigned.contract),
			value: unsigned.value,
			input: unsigned.data.clone(),
			access_list: vec![],
		};

		let key = secp256k1::SecretKey::from_slice(&rand::random::<[u8; 32]>()[..]).unwrap();
		let key_ref = web3::signing::SecretKeyRef::new(&key);

		use web3::signing::Key;
		let sig = key_ref
			.sign(msg.hash().as_bytes(), unsigned.chain_id.into())
			.unwrap();

		let signed_tx = EIP1559Transaction {
			r: H256(sig.r.0),
			s: H256(sig.s.0),
			chain_id: msg.chain_id,
			nonce: msg.nonce,
			max_priority_fee_per_gas: msg.max_priority_fee_per_gas,
			max_fee_per_gas: msg.max_fee_per_gas,
			gas_limit: msg.gas_limit,
			action: msg.action,
			value: msg.value,
			input: msg.input,
			access_list: msg.access_list,
			// EIP-155: sig.v = y_parity + CHAIN_ID * 2 + 35
			odd_y_parity: sig.v - msg.chain_id * 2 + 35 == 1,
		};

		let signed_tx_bytes = rlp::encode(&signed_tx).to_vec();

		assert_ok!(verify_transaction(
			&unsigned,
			&signed_tx_bytes,
			&key_ref.address().0.into()
		));
	}
}
