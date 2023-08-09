//! Types and functions that are common to ethereum.
pub mod api;

pub mod benchmarking;

pub mod deposit_address;

use self::api::tokenizable::Tokenizable;
use crate::*;
pub use cf_primitives::chains::Ethereum;
use cf_primitives::{chains::assets, ChannelId};
use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::ParamType;
pub use ethabi::{
	ethereum_types::{H256, U256},
	Address, Hash as TxHash, Token, Uint, Word,
};
use libsecp256k1::{curve::Scalar, PublicKey, SecretKey};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::ConstBool;
use sp_runtime::{
	traits::{Hash, Keccak256},
	RuntimeDebug,
};
use sp_std::{
	cmp::min,
	convert::{TryFrom, TryInto},
	str, vec,
};

// Reference constants for the chain spec
pub const CHAIN_ID_MAINNET: u64 = 1;
pub const CHAIN_ID_ROPSTEN: u64 = 3;
pub const CHAIN_ID_GOERLI: u64 = 5;
pub const CHAIN_ID_KOVAN: u64 = 42;

impl Chain for Ethereum {
	const NAME: &'static str = "Ethereum";
	type KeyHandoverIsRequired = ConstBool<false>;
	type OptimisticActivation = ConstBool<false>;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = eth::TransactionFee;
	type TrackedData = EthereumTrackedData;
	type ChainAccount = eth::Address;
	type ChainAsset = assets::eth::Asset;
	type EpochStartData = ();
	type DepositFetchId = EthereumFetchId;
	type DepositChannelState = DeploymentStatus;
	type DepositDetails = ();
}

impl ChainCrypto for Ethereum {
	type AggKey = eth::AggKey;
	type Payload = H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionInId = H256;
	// We can't use the hash since we don't know it for Ethereum, as we must select an individaul
	// authority to sign the transaction.
	type TransactionOutId = Self::ThresholdSignature;
	type GovKey = Address;

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

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[codec(mel_bound())]
pub struct EthereumTrackedData {
	pub base_fee: <Ethereum as Chain>::ChainAmount,
	pub priority_fee: <Ethereum as Chain>::ChainAmount,
}

impl Default for EthereumTrackedData {
	#[track_caller]
	fn default() -> Self {
		panic!("You should not use the default chain tracking, as it's meaningless.")
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

impl Display for AggKeyVerificationError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(
			f,
			"{}",
			match self {
				Self::InvalidSignature =>
					"InvalidSignature: The provided signature is not a valid private key",
				Self::InvalidPubkey => "InvalidPubkey: The agg_key is not a valid public key.",
				Self::NoMatch =>
					"NoMatch: The recovered `k_times_g_address` does not match the expected value.",
			}
		)
	}
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

pub fn to_ethereum_address(pubkey: PublicKey) -> eth::Address {
	let [_, k_times_g @ ..] = pubkey.serialize();
	let h = Keccak256::hash(&k_times_g[..]);
	eth::Address::from_slice(&h.0[12..])
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
		let e = self.message_challenge_scalar(msg_hash, k_times_g_address.as_fixed_bytes());

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

	fn param_type() -> ethabi::ParamType {
		ParamType::Tuple(vec![ParamType::Uint(256), ParamType::Uint(8)])
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
	pub max_priority_fee_per_gas: Option<Uint>, // EIP-1559
	pub max_fee_per_gas: Option<Uint>,
	pub gas_limit: Option<Uint>,
	pub contract: Address,
	pub value: Uint,
	pub data: Vec<u8>,
}

impl FeeRefundCalculator<Ethereum> for Transaction {
	fn return_fee_refund(
		&self,
		fee_paid: <Ethereum as Chain>::TransactionFee,
	) -> <Ethereum as Chain>::ChainAmount {
		min(
			self.max_fee_per_gas
				.unwrap_or_default()
				.try_into()
				.expect("In practice `max_fee_per_gas` is always less than u128::MAX"),
			fee_paid.effective_gas_price,
		)
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

	fn check_gas_limit(&self, recovered: Uint) -> Result<(), CheckedTransactionParameter> {
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

	fn check_value(&self, recovered: Uint) -> Result<(), CheckedTransactionParameter> {
		if self.value != recovered {
			return Err(CheckedTransactionParameter::Value)
		}
		Ok(())
	}

	fn check_max_fee_per_gas(&self, recovered: Uint) -> Result<(), CheckedTransactionParameter> {
		if let Some(expected) = self.max_fee_per_gas {
			if expected != recovered {
				return Err(CheckedTransactionParameter::MaxFeePerGas)
			}
		}
		Ok(())
	}

	fn check_max_priority_fee_per_gas(
		&self,
		recovered: Uint,
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

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug, Default)]
pub enum DeploymentStatus {
	#[default]
	Undeployed,
	Pending,
	Deployed,
}

impl ChannelLifecycleHooks for DeploymentStatus {
	/// Addresses that are Pending cannot be fetched.
	fn can_fetch(&self) -> bool {
		*self != Self::Pending
	}

	/// Undeployed addresses need to be marked as Pending until the fetch is made.
	fn on_fetch_scheduled(&mut self) -> bool {
		match self {
			Self::Undeployed => {
				*self = Self::Pending;
				true
			},
			_ => false,
		}
	}

	/// A completed fetch should be in either the pending or deployed state. Confirmation of a fetch
	/// implies that the address is now deployed.
	fn on_fetch_completed(&mut self) -> bool {
		match self {
			Self::Pending => {
				*self = Self::Deployed;
				true
			},
			Self::Deployed => false,
			Self::Undeployed => {
				#[cfg(debug_assertions)]
				{
					panic!("Cannot finalize fetch to an undeployed address")
				}
				#[cfg(not(debug_assertions))]
				{
					log::error!("Cannot finalize fetch to an undeployed address");
					*self = Self::Deployed;
					false
				}
			},
		}
	}

	/// Undeployed Addresses should not be recycled.
	/// Other address types *can* be recycled.
	fn maybe_recycle(self) -> Option<Self> {
		if self == Self::Undeployed {
			None
		} else {
			Some(Self::Deployed)
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub enum EthereumFetchId {
	/// If the contract is not yet deployed, we need to deploy and fetch using the channel id.
	DeployAndFetch(ChannelId),
	/// Once the contract is deployed, we can fetch from the address.
	Fetch(Address),
	/// Fetching is not required for Ethereum deposits into a deployed contract.
	NotRequired,
}

impl From<&DepositChannel<Ethereum>> for EthereumFetchId {
	fn from(channel: &DepositChannel<Ethereum>) -> Self {
		match channel.state {
			DeploymentStatus::Undeployed => EthereumFetchId::DeployAndFetch(channel.channel_id),
			DeploymentStatus::Pending | DeploymentStatus::Deployed =>
				if channel.asset == assets::eth::Asset::Eth {
					EthereumFetchId::NotRequired
				} else {
					EthereumFetchId::Fetch(channel.address)
				},
		}
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

	#[test]
	#[cfg(feature = "runtime-integration-tests")]
	fn test_signature() {
		use rand::{rngs::StdRng, Rng, SeedableRng};
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
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&sig_nonce)).0;

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
		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k)).0;
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
}

#[cfg(test)]
mod lifecycle_tests {
	use super::*;
	const ETH: assets::eth::Asset = assets::eth::Asset::Eth;
	const USDC: assets::eth::Asset = assets::eth::Asset::Usdc;

	macro_rules! expect_deposit_state {
		( $state:expr, $asset:expr, $pat:pat ) => {
			assert!(matches!(
				DepositChannel::<Ethereum> {
					channel_id: Default::default(),
					address: Default::default(),
					asset: $asset,
					state: $state,
				}
				.fetch_id(),
				$pat
			));
		};
	}
	#[test]
	fn eth_deposit_address_lifecycle() {
		// Initial state is undeployed.
		let mut state = DeploymentStatus::default();
		assert_eq!(state, DeploymentStatus::Undeployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EthereumFetchId::DeployAndFetch(..));
		expect_deposit_state!(state, USDC, EthereumFetchId::DeployAndFetch(..));

		// Pending channels can't be fetched from.
		assert!(state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// Trying to schedule the fetch on a pending channel has no effect.
		assert!(!state.on_fetch_scheduled());
		assert_eq!(state, DeploymentStatus::Pending);
		assert!(!state.can_fetch());

		// On completion, the pending channel is now deployed and be fetched from again.
		assert!(state.on_fetch_completed());
		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EthereumFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EthereumFetchId::Fetch(..));

		// Channel is now in its final deployed state and be fetched from at any time.
		assert!(!state.on_fetch_scheduled());
		assert!(state.can_fetch());
		assert!(!state.on_fetch_completed());
		assert!(state.can_fetch());
		expect_deposit_state!(state, ETH, EthereumFetchId::NotRequired);
		expect_deposit_state!(state, USDC, EthereumFetchId::Fetch(..));

		assert_eq!(state, DeploymentStatus::Deployed);
		assert!(!state.on_fetch_scheduled());
	}
}
