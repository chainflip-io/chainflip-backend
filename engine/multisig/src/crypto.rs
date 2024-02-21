#[macro_use]
mod helpers;
pub mod bitcoin;
mod curve25519;
pub mod ed25519;
pub mod eth;
pub mod polkadot;
pub mod secp256k1;
#[cfg(test)]
mod tests;

mod key_id;
pub use key_id::*;

use cf_chains::ChainCrypto;
use generic_array::{typenum::Unsigned, ArrayLength};

use num_derive::FromPrimitive;
use zeroize::{DefaultIsZeroes, ZeroizeOnDrop};

use std::fmt::{Debug, Display};

use generic_array::GenericArray;
use serde::{Deserialize, Serialize};

/// The db uses a static length prefix, that must include the keygen data prefix and the chain tag
pub const CHAIN_TAG_SIZE: usize = std::mem::size_of::<ChainTag>();

/// Upper bound on the size of a point and scalar in bytes, which are useful
/// for estimating size of serialized data. We have tests that (indirectly)
/// check that these are correct.
pub const MAX_POINT_SIZE: usize = 33;
pub const MAX_SCALAR_SIZE: usize = 32;

/// Used as a unique identifier when serializing/deserializing chain specific data.
/// The values are explicitly given and should never be changed.
#[repr(u16)]
#[derive(Clone, Copy, Debug, FromPrimitive)]
pub enum ChainTag {
	Ethereum = 0x0000,
	Polkadot = 0x0001,
	Bitcoin = 0x0002,

	// Ed25519 placeholder
	Ed25519 = 0xffff,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, FromPrimitive)]
pub enum CryptoTag {
	Evm = 0x0000,
	Polkadot = 0x0001,
	Bitcoin = 0x0002,

	// Ed25519 placeholder
	Ed25519 = 0xffff,
}

impl Display for ChainTag {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ChainTag::Ethereum => write!(f, "Ethereum"),
			ChainTag::Polkadot => write!(f, "Polkadot"),
			ChainTag::Bitcoin => write!(f, "Bitcoin"),
			ChainTag::Ed25519 => write!(f, "Ed25519"),
		}
	}
}

impl ChainTag {
	pub const fn to_bytes(self) -> [u8; CHAIN_TAG_SIZE] {
		(self as u16).to_be_bytes()
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeyShare<P: ECPoint> {
	#[serde(bound = "")]
	pub y: P,
	#[serde(bound = "")]
	pub x_i: P::Scalar,
}

// Ideally, we want to use a concrete implementation (like ChaCha20) instead of StdRng
// to prevent it from potentially changing from under us
pub type Rng = rand::rngs::StdRng;

pub trait ECPoint:
	Clone
	+ Copy
	+ Debug
	+ Default
	+ DefaultIsZeroes
	+ 'static
	+ serde::Serialize
	+ for<'de> serde::Deserialize<'de>
	+ std::ops::Mul<Self::Scalar, Output = Self>
	+ for<'a> std::ops::Mul<&'a Self::Scalar, Output = Self>
	+ std::ops::Sub<Output = Self>
	+ std::ops::Add<Output = Self>
	+ std::iter::Sum
	+ PartialEq
	+ Ord
	+ Sync
	+ Send
{
	type Scalar: ECScalar;

	type CompressedPointLength: ArrayLength + Unsigned;

	fn from_scalar(scalar: &Self::Scalar) -> Self;

	fn as_bytes(&self) -> GenericArray<u8, Self::CompressedPointLength>;

	fn point_at_infinity() -> Self;

	fn is_point_at_infinity(&self) -> bool {
		self == &Self::point_at_infinity()
	}
}
pub trait ChainSigning: 'static + Clone + Send + Sync + Debug + PartialEq {
	type CryptoScheme: CryptoScheme;

	type ChainCrypto: cf_chains::ChainCrypto;

	/// Name of the Chain
	const NAME: &'static str;

	/// A unique tag used to identify the chain.
	/// Used in both p2p and database storage.
	const CHAIN_TAG: ChainTag;

	/// The number of ceremonies ahead of the latest authorized ceremony that
	/// are allowed to create unauthorized ceremonies (delayed messages).
	const CEREMONY_ID_WINDOW: u64 = 6000;
}
pub trait CryptoScheme: 'static + Clone + Send + Sync + Debug + PartialEq {
	type Point: ECPoint;

	type Signature: Debug + Clone + PartialEq + Sync + Send;

	type PublicKey: CanonicalEncoding + Debug + Clone + Sync + Send;

	type SigningPayload: Display + Debug + Sync + Send + Clone + PartialEq + Eq + AsRef<[u8]>;

	/// A unique tag used to identify the crypto scheme.
	const CRYPTO_TAG: CryptoTag;

	/// Friendly name of the scheme used for logging
	const NAME: &'static str;

	fn build_signature(
		z: <Self::Point as ECPoint>::Scalar,
		group_commitment: Self::Point,
	) -> Self::Signature;

	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		payload: &Self::SigningPayload,
	) -> <Self::Point as ECPoint>::Scalar;

	/// Build challenge response using our key share
	fn build_response(
		nonce: <Self::Point as ECPoint>::Scalar,
		nonce_commitment: Self::Point,
		private_key: &<Self::Point as ECPoint>::Scalar,
		challenge: <Self::Point as ECPoint>::Scalar,
	) -> <Self::Point as ECPoint>::Scalar;

	/// Check that a party's challenge response is valid
	/// w.r.t their public key share
	/// (See step 7.b in Figure 3, page 15 of https://eprint.iacr.org/2020/852.pdf)
	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as ECPoint>::Scalar,
		commitment: &Self::Point,
		group_commitment: &Self::Point,
		challenge: &<Self::Point as ECPoint>::Scalar,
		signature_response: &<Self::Point as ECPoint>::Scalar,
	) -> bool;

	fn verify_signature(
		signature: &Self::Signature,
		public_key: &Self::PublicKey,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()>;

	/// Convert a point to a public key.
	fn pubkey_from_point(pubkey_point: &Self::Point) -> Self::PublicKey;

	// Only relevant for ETH and BTC keys, which are the only
	// implementations that are expected to overwrite this
	fn is_pubkey_compatible(_pubkey: &Self::Point) -> bool {
		true
	}

	#[cfg(feature = "test")]
	fn signing_payload_for_test() -> Self::SigningPayload;

	#[cfg(feature = "test")]
	/// Get an invalid signature for testing purposes
	fn signature_for_test() -> Self::Signature {
		use rand::{rngs::StdRng, SeedableRng};
		let scalar = <Self::Point as ECPoint>::Scalar::random(&mut StdRng::from_seed([0_u8; 32]));
		let point = <Self::Point as ECPoint>::from_scalar(&scalar);
		Self::build_signature(scalar, point)
	}
}

pub trait ECScalar:
	Clone
	+ Debug
	+ Sized
	+ Default
	+ serde::Serialize
	+ for<'de> serde::Deserialize<'de>
	+ for<'a> std::ops::Mul<&'a Self, Output = Self>
	+ for<'a> std::ops::Add<&'a Self, Output = Self>
	+ std::ops::Mul<Output = Self>
	+ std::ops::Add<Output = Self>
	+ std::ops::Sub<Output = Self>
	+ std::iter::Sum
	+ zeroize::Zeroize
	+ PartialEq
	+ Ord
	+ Sync
	+ Send
	+ ZeroizeOnDrop
	+ std::convert::From<u32>
{
	fn random(rng: &mut Rng) -> Self;

	fn from_bytes_mod_order(x: &[u8; 32]) -> Self;

	fn zero() -> Self;

	fn invert(&self) -> Option<Self>;
}

#[cfg(test)]
pub fn generate_single_party_signature<C: CryptoScheme>(
	secret_key: &<C::Point as ECPoint>::Scalar,
	payload: &C::SigningPayload,
	rng: &mut Rng,
) -> C::Signature {
	use super::client::signing::generate_schnorr_response;

	let public_key = C::Point::from_scalar(secret_key);

	let nonce = <C::Point as ECPoint>::Scalar::random(rng);

	let r = C::Point::from_scalar(&nonce);

	let sigma = generate_schnorr_response::<C>(secret_key, public_key, r, nonce, payload);

	C::build_signature(sigma, r)
}

pub trait SignatureToThresholdSignature<C: ChainCrypto> {
	fn to_threshold_signature(&self) -> C::ThresholdSignature;
}
