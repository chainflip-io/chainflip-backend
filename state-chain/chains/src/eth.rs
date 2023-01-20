//! Types and functions that are common to ethereum.
pub mod api;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

pub mod ingress_address;

use crate::*;
pub use cf_primitives::chains::{assets, Ethereum};
use cf_primitives::KeyId;
use codec::{Decode, Encode, MaxEncodedLen};
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use libsecp256k1::{curve::Scalar, PublicKey, SecretKey};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::{
	cmp::min,
	convert::{TryFrom, TryInto},
	str, vec,
};

use self::api::EthereumReplayProtection;

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
pub const CHAIN_ID_GOERLI: u64 = 5;
pub const CHAIN_ID_KOVAN: u64 = 42;

impl Chain for Ethereum {
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = eth::TransactionFee;
	type TrackedData = eth::TrackedData<Self>;
	type ChainAccount = eth::Address;
	type ChainAsset = assets::eth::Asset;
	type EpochStartData = ();
}

impl ChainCrypto for Ethereum {
	type KeyId = KeyId;
	type AggKey = eth::AggKey;
	type Payload = eth::H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionId = eth::H256;
	type GovKey = eth::Address;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		agg_key
			.verify(payload.as_fixed_bytes(), signature)
			.map_err(|e| log::debug!("Ethereum signature verification failed: {:?}.", e))
			.is_ok()
	}

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		H256(Blake2_256::hash(&agg_key.to_pubkey_compressed()))
	}
}

//--------------------------//
pub trait Tokenizable {
	fn tokenize(self) -> Token;
}

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[codec(mel_bound())]
pub struct TrackedData<C: Chain> {
	pub block_height: C::ChainBlockNumber,
	pub base_fee: C::ChainAmount,
	pub priority_fee: C::ChainAmount,
}

impl<C: Chain> Age<C> for TrackedData<C> {
	fn birth_block(&self) -> <C as Chain>::ChainBlockNumber {
		self.block_height
	}
}

/// The `SigData` struct used for threshold signatures in the smart contracts.
/// See [here](https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol).
#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SigData {
	/// The address of the Key Manager contract, to prevent replay attacks
	key_manager_address: Address,
	/// The ID of the chain we're broadcasting to, to prevent x-chain replays
	chain_id: Uint,
	/// The message hash aka. payload to be signed over.
	msg_hash: H256,
	/// The Schnorr signature.
	sig: Uint,
	/// The nonce value for the AggKey. Each Signature over an AggKey should have a unique nonce to
	/// prevent replay attacks.
	nonce: Uint,
	/// The address value derived from the random nonce value `k`. Also known as
	/// `nonceTimesGeneratorAddress`.
	///
	/// Note this is unrelated to the `nonce` above. The nonce in the context of
	/// `nonceTimesGeneratorAddress` is a generated as part of each signing round (ie. as part of
	/// the Schnorr signature) to prevent certain classes of cryptographic attacks.
	k_times_g_address: Address,
}

impl MaxEncodedLen for SigData {
	fn max_encoded_len() -> usize {
		<[u8; 20]>::max_encoded_len() * 2 // 2 x Addresses
		+ <[u64; 4]>::max_encoded_len() * 3 // 3 x Uint
		+ <[u8; 32]>::max_encoded_len() // H256
	}
}

impl SigData {
	/// Initiate a new `SigData` with given nonce value
	pub fn new_empty(replay_protection: EthereumReplayProtection) -> Self {
		Self {
			key_manager_address: replay_protection.key_manager_address.into(),
			chain_id: replay_protection.chain_id.into(),
			nonce: replay_protection.nonce.into(),
			..Default::default()
		}
	}

	/// Used for migrating from the old `SigData` struct.
	pub fn from_legacy(msg_hash: H256, sig: Uint, nonce: Uint, k_times_g_address: Address) -> Self {
		Self { msg_hash, sig, nonce, k_times_g_address, ..Default::default() }
	}

	/// Inserts the `msg_hash` value derived from the provided calldata.
	pub fn insert_msg_hash_from(&mut self, calldata: &[u8]) {
		self.msg_hash = H256(Keccak256::hash(calldata).0);
	}

	/// Add the actual signature. This method does no verification.
	pub fn insert_signature(&mut self, schnorr: &SchnorrVerificationComponents) {
		self.sig = schnorr.s.into();
		self.k_times_g_address = schnorr.k_times_g_address.into();
	}

	/// Get the inner signature components as a `SchnorrVerificationComponents`.
	pub fn get_signature(&self) -> SchnorrVerificationComponents {
		SchnorrVerificationComponents {
			s: self.sig.into(),
			k_times_g_address: self.k_times_g_address.into(),
		}
	}

	pub fn is_signed(&self) -> bool {
		self.sig != Default::default() && self.k_times_g_address != Default::default()
	}
}

impl Tokenizable for SigData {
	fn tokenize(self) -> Token {
		Token::Tuple(vec![
			Token::Address(self.key_manager_address),
			Token::Uint(self.chain_id),
			Token::Uint(self.msg_hash.0.into()),
			Token::Uint(self.sig),
			Token::Uint(self.nonce),
			Token::Address(self.k_times_g_address),
		])
	}
}

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum AggKeyVerificationError {
	/// The provided signature (aka. `s`) is not a valid private key.
	InvalidSignature,
	/// The agg_key is not a valid public key.
	InvalidPubkey,
	/// The recovered `k_times_g_address` does not match the expected value.
	NoMatch,
}

/// A parity bit can be either odd or even, but can have different representations depending on its
/// use. Ethereum generaly assumes `0` or `1` but the standard serialization format used in most
/// libraries assumes `2` or `3`.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
)]
pub enum ParityBit {
	Odd,
	Even,
}

impl ParityBit {
	/// Returns `true` if the parity bit is odd, otherwise `false`.
	pub fn is_odd(&self) -> bool {
		matches!(self, Self::Odd)
	}

	/// Returns `true` if the parity bit is even, otherwise `false`.
	pub fn is_even(&self) -> bool {
		matches!(self, Self::Even)
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

impl Default for ParityBit {
	/// Default ParityBit is even (zero)
	fn default() -> Self {
		ParityBit::Even
	}
}

/// For encoding the `Key` type as defined in <https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/interfaces/IShared.sol>
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(
	Default,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
)]
pub struct AggKey {
	/// X coordinate of the public key as a 32-byte array.
	pub pub_key_x: [u8; 32],
	/// The parity bit can be odd or even.
	pub pub_key_y_parity: ParityBit,
}

pub fn to_ethereum_address(pubkey: PublicKey) -> [u8; 20] {
	let [_, k_times_g @ ..] = pubkey.serialize();
	let h = Keccak256::hash(&k_times_g[..]);
	let mut res = [0u8; 20];
	res.copy_from_slice(&h.0[12..]);
	res
}

impl AggKey {
	/// Convert from compressed `[y, x]` coordinates where y==2 means "even" and y==3 means "odd".
	///
	/// Note that the ethereum contract expects y==0 for "even" and y==1 for "odd". We convert to
	/// the required 0 / 1 representation by subtracting 2 from the supplied values, so if the
	/// source format doesn't conform to the expected 2/3 even/odd convention, bad things will
	/// happen.
	pub fn from_pubkey_compressed(bytes: [u8; 33]) -> Self {
		let [pub_key_y_parity, pub_key_x @ ..] = bytes;
		let pub_key_y_parity = if pub_key_y_parity == 2 { ParityBit::Even } else { ParityBit::Odd };
		Self { pub_key_x, pub_key_y_parity }
	}

	/// Create a public `AggKey` from the private key component.
	pub fn from_private_key_bytes(agg_key_private: [u8; 32]) -> Self {
		let secret_key = SecretKey::parse(&agg_key_private).expect("Valid private key");
		AggKey::from_pubkey_compressed(
			PublicKey::from_secret_key(&secret_key).serialize_compressed(),
		)
	}

	/// Convert to 'compressed pubkey` format where a leading `2` means 'even parity bit' and a
	/// leading `3` means 'odd'.
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
	/// Note that the result is not reduced to group order at this point, so we need to be careful
	/// when converting the result to a scalar.
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
	pub fn message_challenge(&self, msg_hash: &[u8; 32], k_times_g_address: &[u8; 20]) -> [u8; 32] {
		// Note the contract expects a packed u8 of 0 (even) or 1 (odd).
		let parity_bit_uint_packed = match self.pub_key_y_parity {
			ParityBit::Odd => 1u8,
			ParityBit::Even => 0u8,
		};
		Keccak256::hash(
			[&self.pub_key_x[..], &[parity_bit_uint_packed], &msg_hash[..], &k_times_g_address[..]]
				.concat()
				.as_ref(),
		)
		.into()
	}

	fn message_challenge_scalar(
		&self,
		msg_hash: &[u8; 32],
		k_times_g_address: &[u8; 20],
	) -> Scalar {
		let challenge = self.message_challenge(msg_hash, k_times_g_address);
		let mut s = Scalar::default();
		// Even if this "overflows", the scalar is reduced to the group order,
		// which is what the signature scheme (the contract) expects
		let _overflowed = s.set_b32(&challenge);
		s
	}

	/// Sign a message, using a secret key, and a signature nonce
	#[cfg(any(feature = "runtime-integration-tests", feature = "runtime-benchmarks"))]
	pub fn sign(&self, msg_hash: &[u8; 32], secret: &SecretKey, sig_nonce: &SecretKey) -> [u8; 32] {
		use sp_std::ops::Neg;

		// Compute s = (k - d * e) % Q
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(sig_nonce));
		let e = self.message_challenge_scalar(msg_hash, &k_times_g_address);

		let d: Scalar = (*secret).into();
		let k: Scalar = (*sig_nonce).into();
		let signature: Scalar = k + (e * d).neg();

		signature.b32()
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
		// We don't have nonce, we have k_times_g_address so will instead verify like this:
		//     encode_addr(msgChallenge * signingPubKey + signature * generator) ==
		// encode_addr(nonce * generator) Simplified:
		//     encode_addr(msgChallenge * signingPubKey + signature * generator) ==
		// k_times_g_address
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
			let mut pubkey = PublicKey::parse_compressed(&self.to_pubkey_compressed())
				.map_err(|_| AggKeyVerificationError::InvalidPubkey)?;

			// Convert the message challenge to a Secret Key value so it can be multiplied with the
			// point.
			let challenge = self.message_challenge_scalar(msg_hash, &sig.k_times_g_address);
			// This will fail for a "zero" challenge, which is not expected and we might as well
			// consider the signature invalid
			let challenge_sk = SecretKey::try_from(challenge)
				.map_err(|_| AggKeyVerificationError::InvalidSignature)?;

			// Multiply scalar and point. This can only fail if challenge is "zero",
			// which it can't be by construction above
			pubkey.tweak_mul_assign(&challenge_sk).expect("challenge can't be zero");
			pubkey
		};

		// Add two pubkeys. The signature is considered invalid if the result is
		// a point at infinity (which is the only way `combine` can fail)
		let k_times_g_recovered = PublicKey::combine(&[challenge_times_pubkey, s_times_g])
			.map_err(|_| AggKeyVerificationError::InvalidSignature)?;

		// We now have the recovered value for k_times_g, however we only have a
		// k_times_g_address to compare against. So we need to convert our recovered k_times_g to
		// an Ethereum address to compare against our expected value.
		let k_times_g_hash_recovered = Keccak256::hash(&k_times_g_recovered.serialize()[1..]);

		// The signature is valid if the recovered value matches the provided one.
		if k_times_g_hash_recovered[12..] == sig.k_times_g_address {
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
/// Conversion fails *unless* the first byte is the y parity byte encoded as `2` or `3` *and* the
/// total length of the slice is 33 bytes.
impl TryFrom<&[u8]> for AggKey {
	type Error = &'static str;

	fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
		if bytes.len() != 33 {
			log::error!("Invalid aggKey format: Should be 33 bytes total, got {}", bytes.len());
			return Err("Invalid aggKey format: Should be 33 bytes total.")
		}

		let pub_key_y_parity = match bytes[0] {
			2 => Ok(ParityBit::Even),
			3 => Ok(ParityBit::Odd),
			invalid => {
				log::error!(
					"Invalid aggKey format: Leading byte should be 2 or 3, got {}",
					invalid,
				);

				Err("Invalid aggKey format: Leading byte should be 2 or 3")
			},
		}?;

		let pub_key_x: [u8; 32] = bytes[1..].try_into().map_err(|e| {
			log::error!("Invalid aggKey format: {:?}", e);
			"Invalid aggKey format: x coordinate should be 32 bytes."
		})?;

		Ok(Self { pub_key_x, pub_key_y_parity })
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

#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct SchnorrVerificationComponents {
	/// Scalar component
	pub s: [u8; 32],
	/// The challenge, expressed as a truncated keccak hash of a pair of coordinates.
	pub k_times_g_address: [u8; 20],
}

/// Errors that can occur when verifying an Ethereum transaction.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum TransactionVerificationError {
	/// The transaction's chain id is invalid.
	InvalidChainId,
	/// The derived recovery id is invalid.
	InvalidRecoveryId,
	/// The signed payload was not valid rlp-encoded data.
	InvalidRlp,
	/// The transaction signature was invalid.
	InvalidSignature,
	/// The recovered address does not match the provided one.
	NoMatch,
	/// The signed transaction parameters do not all match those of the unsigned transaction.
	InvalidParam(CheckedTransactionParameter),
}

/// Parameters that are checked as part of Ethereum transaction verification.
#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum CheckedTransactionParameter {
	ChainId,
	GasLimit,
	Data,
	Value,
	ContractAddress,
	Action,
	MaxFeePerGas,
	MaxPriorityFeePerGas,
}

impl From<CheckedTransactionParameter> for TransactionVerificationError {
	fn from(p: CheckedTransactionParameter) -> Self {
		Self::InvalidParam(p)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct TransactionFee {
	// priority + base
	pub effective_gas_price: EthAmount,
	pub gas_used: u128,
}

/// Required information to construct and sign an ethereum transaction. Equivalent to
/// [ethereum::EIP1559TransactionMessage] with the following fields omitted: nonce,
///
/// The signer will need to add its account nonce and then sign and rlp-encode the transaction.
///
/// We assume the access_list (EIP-2930) is not required.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct Transaction {
	pub chain_id: u64,
	pub max_priority_fee_per_gas: Option<U256>, // EIP-1559
	pub max_fee_per_gas: Option<U256>,
	pub gas_limit: Option<U256>,
	pub contract: Address,
	pub value: U256,
	pub data: Vec<u8>,
}

impl FeeRefundCalculator<Ethereum> for Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Ethereum as Chain>::TransactionFee,
	) -> <Ethereum as Chain>::ChainAmount {
		min(self.max_fee_per_gas.unwrap_or_default().as_u128(), fee_paid.effective_gas_price)
			.saturating_mul(fee_paid.gas_used)
	}
}

impl Transaction {
	fn check_contract(
		&self,
		recovered: ethereum::TransactionAction,
	) -> Result<(), CheckedTransactionParameter> {
		match recovered {
			ethereum::TransactionAction::Call(address) => {
				if address.as_bytes() != self.contract.as_bytes() {
					return Err(CheckedTransactionParameter::ContractAddress)
				}
			},
			ethereum::TransactionAction::Create => return Err(CheckedTransactionParameter::Action),
		};
		Ok(())
	}

	fn check_gas_limit(&self, recovered: U256) -> Result<(), CheckedTransactionParameter> {
		if let Some(expected) = self.gas_limit {
			if expected != recovered {
				return Err(CheckedTransactionParameter::GasLimit)
			}
		}
		Ok(())
	}

	fn check_chain_id(&self, recovered: u64) -> Result<(), CheckedTransactionParameter> {
		if self.chain_id != recovered {
			return Err(CheckedTransactionParameter::ChainId)
		}
		Ok(())
	}

	fn check_data(&self, recovered: Vec<u8>) -> Result<(), CheckedTransactionParameter> {
		if self.data != recovered {
			return Err(CheckedTransactionParameter::Data)
		}
		Ok(())
	}

	fn check_value(&self, recovered: U256) -> Result<(), CheckedTransactionParameter> {
		if self.value != recovered {
			return Err(CheckedTransactionParameter::Value)
		}
		Ok(())
	}

	fn check_max_fee_per_gas(&self, recovered: U256) -> Result<(), CheckedTransactionParameter> {
		if let Some(expected) = self.max_fee_per_gas {
			if expected != recovered {
				return Err(CheckedTransactionParameter::MaxFeePerGas)
			}
		}
		Ok(())
	}

	fn check_max_priority_fee_per_gas(
		&self,
		recovered: U256,
	) -> Result<(), CheckedTransactionParameter> {
		if let Some(expected) = self.max_priority_fee_per_gas {
			if expected != recovered {
				return Err(CheckedTransactionParameter::MaxPriorityFeePerGas)
			}
		}
		Ok(())
	}

	/// Returns an error if any of the recovered transactoin parameters do not match those specified
	/// in the original [Transaction].
	///
	/// See [CheckedTransactionParameter].
	pub fn match_against_recovered(
		&self,
		recovered: ethereum::TransactionV2,
	) -> Result<(), TransactionVerificationError> {
		match recovered {
			ethereum::TransactionV2::Legacy(tx) => {
				let msg: ethereum::LegacyTransactionMessage = tx.into();
				let chain_id = msg.chain_id.ok_or(CheckedTransactionParameter::ChainId)?;
				self.check_chain_id(chain_id)?;
				self.check_gas_limit(msg.gas_limit)?;
				self.check_data(msg.input)?;
				self.check_value(msg.value)?;
				self.check_contract(msg.action)?;
			},
			ethereum::TransactionV2::EIP2930(tx) => {
				let msg: ethereum::EIP2930TransactionMessage = tx.into();
				self.check_chain_id(msg.chain_id)?;
				self.check_gas_limit(msg.gas_limit)?;
				self.check_data(msg.input)?;
				self.check_value(msg.value)?;
				self.check_contract(msg.action)?;
			},
			ethereum::TransactionV2::EIP1559(tx) => {
				let msg: ethereum::EIP1559TransactionMessage = tx.into();
				self.check_chain_id(msg.chain_id)?;
				self.check_gas_limit(msg.gas_limit)?;
				self.check_max_fee_per_gas(msg.max_fee_per_gas)?;
				self.check_max_priority_fee_per_gas(msg.max_priority_fee_per_gas)?;
				self.check_data(msg.input)?;
				self.check_value(msg.value)?;
				self.check_contract(msg.action)?;
			},
		};

		Ok(())
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Default)]
pub struct TransactionHash(H256);
impl core::fmt::Debug for TransactionHash {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		f.write_fmt(format_args!("{:#?}", self.0))
	}
}

impl From<H256> for TransactionHash {
	fn from(x: H256) -> Self {
		Self(x)
	}
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

#[cfg(any(test, feature = "runtime-benchmarks"))]
pub mod sig_constants {
	/*
		The below constants have been derived from integration tests with the KeyManager contract.

		In order to check if verification works, we need to use this to construct the AggKey and `SigData` as we
		normally would when submitting a function call to a threshold-signature-protected smart contract.
	*/
	pub const AGG_KEY_PRIV: [u8; 32] =
		hex_literal::hex!("fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba");
	pub const AGG_KEY_PUB: [u8; 33] =
		hex_literal::hex!("0331b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae");
	pub const MSG_HASH: [u8; 32] =
		hex_literal::hex!("2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e");
	pub const SIG: [u8; 32] =
		hex_literal::hex!("beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538");
	pub const SIG_NONCE: [u8; 32] =
		hex_literal::hex!("d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285");
}

#[cfg(test)]
mod verification_tests {
	use crate::eth::sig_constants::{AGG_KEY_PRIV, AGG_KEY_PUB, MSG_HASH, SIG, SIG_NONCE};

	use super::*;
	use frame_support::{assert_err, assert_ok};
	use libsecp256k1::{PublicKey, SecretKey};
	use rand::{prelude::*, SeedableRng};
	use Keccak256;

	#[test]
	#[cfg(feature = "runtime-integration-tests")]
	fn test_signature() {
		// Message to sign over
		let msg: [u8; 32] = Keccak256::hash(b"Whats it going to be then, eh?")
			.as_bytes()
			.try_into()
			.unwrap();

		// Create an agg key
		let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(100).gen();
		let agg_key_secret_key = SecretKey::parse(&agg_key_priv).unwrap();

		// Signature nonce
		let sig_nonce: [u8; 32] = StdRng::seed_from_u64(200).gen();
		let sig_nonce = SecretKey::parse(&sig_nonce).unwrap();
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&sig_nonce));

		// Public agg key
		let agg_key = AggKey::from_private_key_bytes(agg_key_priv);

		// Sign over message
		let signature = agg_key.sign(&msg, &agg_key_secret_key, &sig_nonce);

		// Construct components for verification
		let sig = SchnorrVerificationComponents { s: signature, k_times_g_address };

		// Verify signature
		assert_ok!(agg_key.verify(&msg, &sig));
	}

	#[test]
	fn test_schnorr_signature_verification() {
		let agg_key = AggKey::from_private_key_bytes(AGG_KEY_PRIV);
		assert_eq!(agg_key.to_pubkey_compressed(), AGG_KEY_PUB);

		let k = SecretKey::parse(&SIG_NONCE).expect("Valid signature nonce");
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		let sig = SchnorrVerificationComponents { s: SIG, k_times_g_address };

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
		assert_err!(bad_agg_key.verify(&MSG_HASH, &sig), AggKeyVerificationError::NoMatch);

		// Providing the wrong signature should fail.
		assert!(agg_key
			.verify(
				&MSG_HASH,
				&SchnorrVerificationComponents { s: SIG.map(|i| i + 1), k_times_g_address }
			)
			.is_err(),);

		// Providing the wrong nonce should fail.
		assert_err!(
			agg_key.verify(
				&MSG_HASH,
				&SchnorrVerificationComponents {
					s: SIG,
					k_times_g_address: k_times_g_address.map(|i| i + 1),
				}
			),
			AggKeyVerificationError::NoMatch
		);
	}

	#[test]
	fn test_max_encoded_len() {
		cf_test_utilities::ensure_max_encoded_len_is_exact::<SigData>();
	}
}
