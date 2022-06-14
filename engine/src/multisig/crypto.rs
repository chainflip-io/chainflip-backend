#[macro_use]
mod helpers;
pub mod curve25519_ristretto;
pub mod eth;
pub mod polkadot;
pub mod secp255k1;

use generic_array::{typenum::Unsigned, ArrayLength};

pub use curv::{arithmetic::traits::Converter as BigIntConverter, BigInt};
use zeroize::{DefaultIsZeroes, ZeroizeOnDrop};

use std::fmt::Debug;

use generic_array::GenericArray;
use serde::{Deserialize, Serialize};

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

    // Only relevant for ETH contract keys
    fn is_compatible(&self) -> bool {
        true
    }
}

pub trait CryptoScheme: 'static {
    type Point: ECPoint;

    type Signature: Debug
        + Clone
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + Sync
        + Send;

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

    fn from_bytes(x: &[u8; 32]) -> Self;

    fn zero() -> Self;

    fn invert(&self) -> Option<Self>;
}
