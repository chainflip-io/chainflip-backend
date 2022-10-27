#[macro_use]
mod helpers;
pub mod curve25519_ristretto;
pub mod eth;
pub mod polkadot;
pub mod secp256k1;

use generic_array::{typenum::Unsigned, ArrayLength};

use num_derive::FromPrimitive;
use zeroize::{DefaultIsZeroes, ZeroizeOnDrop};

use std::fmt::Debug;

use generic_array::GenericArray;
use serde::{Deserialize, Serialize};

use super::KeyId;

/// The db uses a static length prefix, that must include the keygen data prefix and the chain tag
pub const CHAIN_TAG_SIZE: usize = std::mem::size_of::<ChainTag>();

/// Used as a unique identifier when serializing/deserializing chain specific data.
/// The values are explicitly given and should never be changed.
#[repr(u16)]
#[derive(Clone, Copy, Debug, FromPrimitive)]
pub enum ChainTag {
	Ethereum = 0x0000,
	Polkadot = 0x0001,
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
// to prevent it from potentially changing from under us (but it needs to be compatible
// with rand_legacy)
pub type Rng = rand_legacy::rngs::StdRng;

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
	+ Sync
	+ Send
{
	type Scalar: ECScalar;

	type CompressedPointLength: ArrayLength<u8> + Unsigned;

	fn from_scalar(scalar: &Self::Scalar) -> Self;

	fn as_bytes(&self) -> GenericArray<u8, Self::CompressedPointLength>;

	fn point_at_infinity() -> Self;

	fn is_point_at_infinity(&self) -> bool {
		self == &Self::point_at_infinity()
	}
}

pub trait Verifiable {
	fn verify(&self, key_id: &KeyId, message: &[u8; 32]) -> anyhow::Result<()>;
}

pub trait CryptoScheme: 'static {
	type Point: ECPoint;

	type Signature: Verifiable
		+ Debug
		+ Clone
		+ PartialEq
		+ serde::Serialize
		+ for<'de> serde::Deserialize<'de>
		+ Sync
		+ Send;

	/// Friendly name of the scheme used for logging
	const NAME: &'static str;

	/// A unique tag used to identify the chain.
	/// Used in both p2p and database storage.
	const CHAIN_TAG: ChainTag;

	fn build_signature(
		z: <Self::Point as ECPoint>::Scalar,
		group_commitment: Self::Point,
	) -> Self::Signature;

	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		msg_hash: &[u8; 32],
	) -> <Self::Point as ECPoint>::Scalar;

	fn build_response(
		nonce: <Self::Point as ECPoint>::Scalar,
		private_key: &<Self::Point as ECPoint>::Scalar,
		challenge: <Self::Point as ECPoint>::Scalar,
	) -> <Self::Point as ECPoint>::Scalar;

	// Only relevant for ETH contract keys, which is the only
	// implementation that is expected to overwrite this
	fn is_pubkey_compatible(_pubkey: &Self::Point) -> bool {
		true
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
