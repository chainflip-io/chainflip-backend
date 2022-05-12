use generic_array::GenericArray;
use zeroize::{DefaultIsZeroes, Zeroize, ZeroizeOnDrop};

use super::{CryptoScheme, ECPoint, ECScalar, Rng};
use serde::{Deserialize, Serialize};

use curv::{
    arithmetic::Converter,
    elliptic::curves::{
        secp256_k1::{Secp256k1Point, Secp256k1Scalar},
        ECPoint as CurvECPoint, ECScalar as CurvECScalar,
    },
    BigInt,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point(pub Secp256k1Point);

impl std::ops::Mul<Scalar> for Point {
    type Output = Point;

    fn mul(self, rhs: Scalar) -> Self::Output {
        Point(self.0.scalar_mul(&rhs.0))
    }
}

impl std::ops::Mul<&Scalar> for Point {
    type Output = Point;

    fn mul(self, rhs: &Scalar) -> Self::Output {
        Point(self.0.scalar_mul(&rhs.0))
    }
}

impl std::ops::Mul<&Scalar> for &Point {
    type Output = Point;

    fn mul(self, rhs: &Scalar) -> Self::Output {
        Point(self.0.scalar_mul(&rhs.0))
    }
}

impl std::ops::Add for &Point {
    type Output = Point;

    fn add(self, rhs: Self) -> Self::Output {
        Point(self.0.add_point(&rhs.0))
    }
}

impl std::ops::Add for Point {
    type Output = Point;

    fn add(self, rhs: Self) -> Self::Output {
        <&Point>::add(&self, &rhs)
    }
}

impl std::ops::Sub for Point {
    type Output = Point;

    fn sub(self, rhs: Self) -> Self::Output {
        Point(self.0.sub_point(&rhs.0))
    }
}

impl std::ops::Sub<Point> for &Point {
    type Output = Point;

    fn sub(self, rhs: Point) -> Self::Output {
        *self - rhs
    }
}

impl std::iter::Sum for Point {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Point(Secp256k1Point::zero()), |a, b| a + b)
    }
}

impl Default for Point {
    fn default() -> Self {
        Point(Secp256k1Point::zero())
    }
}

impl DefaultIsZeroes for Point {}

impl ECPoint for Point {
    type Scalar = Scalar;
    type Underlying = secp256k1::PublicKey;
    type CompressedPointLength = <Secp256k1Point as CurvECPoint>::CompressedPointLength;

    fn from_scalar(scalar: &Scalar) -> Self {
        Point(Secp256k1Point::generator().scalar_mul(&scalar.0))
    }

    fn get_element(&self) -> Self::Underlying {
        // TODO: ensure that we don't create points at infinity
        // (we might want to sanitize p2p data)
        self.0
            .underlying_ref()
            .expect("unexpected point at infinity")
            .0
    }

    fn as_bytes(&self) -> GenericArray<u8, Self::CompressedPointLength> {
        self.0.serialize_compressed()
    }

    fn is_point_at_infinity(&self) -> bool {
        self.0.is_zero()
    }

    fn is_compatible(&self) -> bool {
        // Check if the public key's x coordinate is smaller than "half secp256k1's order",
        // which is a requirement imposed by the Key Manager contract
        let pk = self.get_element();
        let pubkey = cf_chains::eth::AggKey::from(&pk);

        let x = BigInt::from_bytes(&pubkey.pub_key_x);
        let half_order = BigInt::from_bytes(&secp256k1::constants::CURVE_ORDER) / 2 + 1;

        x < half_order
    }
}

impl Serialize for Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.0.serialize_compressed().as_ref())
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = Vec::deserialize(deserializer)?;

        Secp256k1Point::deserialize(&bytes)
            .map(Point)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
impl Point {
    pub fn random(rng: &mut Rng) -> Self {
        Point::from_scalar(&Scalar::random(rng))
    }
}

// ----------- scalar --------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Scalar(Secp256k1Scalar);

impl Default for Scalar {
    fn default() -> Self {
        Scalar::zero()
    }
}

impl ECScalar for Scalar {
    fn random(rng: &mut Rng) -> Self {
        use curv::elliptic::curves::secp256_k1::SK;

        Scalar(Secp256k1Scalar::from_underlying(Some(SK(
            secp256k1::SecretKey::new(rng),
        ))))
    }

    fn from_bytes(x: &[u8; 32]) -> Self {
        Scalar(CurvECScalar::from_bigint(&BigInt::from_bytes(x)))
    }

    fn from_usize(x: usize) -> Self {
        Scalar(CurvECScalar::from_bigint(&BigInt::from(x as u64)))
    }

    fn zero() -> Self {
        Scalar(Secp256k1Scalar::zero())
    }

    fn invert(&self) -> Option<Self> {
        self.0.invert().map(Scalar)
    }
}

impl ZeroizeOnDrop for Scalar {}

impl Drop for Scalar {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl zeroize::Zeroize for Scalar {
    fn zeroize(&mut self) {
        // Secp256k1Scalar doesn't expose a way to "zeroize" it apart from dropping, so have
        // to do it manually (I think assigning a different value would be sufficient to drop
        // and zeroize the value, but we are not 100% sure that it won't get optimised away).
        use core::sync::atomic;
        unsafe { std::ptr::write_volatile(&mut self.0, Secp256k1Scalar::zero()) };
        atomic::compiler_fence(atomic::Ordering::SeqCst);
    }
}

impl std::ops::Sub for Scalar {
    type Output = Scalar;

    fn sub(self, rhs: Self) -> Self::Output {
        &self - &rhs
    }
}

impl std::ops::Sub for &Scalar {
    type Output = Scalar;

    fn sub(self, rhs: Self) -> Self::Output {
        Scalar(self.0.sub(&rhs.0))
    }
}

impl std::ops::Mul<&Scalar> for Scalar {
    type Output = Scalar;

    fn mul(self, rhs: &Scalar) -> Self::Output {
        &self * rhs
    }
}

impl std::ops::Mul for Scalar {
    type Output = Scalar;

    fn mul(self, rhs: Self) -> Self::Output {
        &self * &rhs
    }
}

impl std::ops::Mul for &Scalar {
    type Output = Scalar;

    fn mul(self, rhs: Self) -> Self::Output {
        Scalar(self.0.mul(&rhs.0))
    }
}

impl std::ops::Add for Scalar {
    type Output = Scalar;

    fn add(self, rhs: Self) -> Self::Output {
        <&Scalar>::add(&self, &rhs)
    }
}

impl std::ops::Add for &Scalar {
    type Output = Scalar;

    fn add(self, rhs: Self) -> Self::Output {
        Scalar(self.0.add(&rhs.0))
    }
}

impl std::ops::Add<&Scalar> for Scalar {
    type Output = Scalar;

    fn add(self, rhs: &Scalar) -> Self::Output {
        &self + rhs
    }
}

impl std::iter::Sum for Scalar {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Scalar::zero(), |a, b| a + b)
    }
}

impl Serialize for Scalar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.0.serialize().as_ref())
    }
}

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = Vec::deserialize(deserializer)?;

        Secp256k1Scalar::deserialize(&bytes)
            .map(Scalar)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthSchnorrSignature {
    /// Scalar component
    pub s: [u8; 32],
    /// Point component (commitment)
    pub r: secp256k1::PublicKey,
}

impl From<EthSchnorrSignature> for cf_chains::eth::SchnorrVerificationComponents {
    fn from(cfe_sig: EthSchnorrSignature) -> Self {
        use crate::eth::utils::pubkey_to_eth_addr;

        Self {
            s: cfe_sig.s,
            k_times_g_address: pubkey_to_eth_addr(cfe_sig.r),
        }
    }
}

impl Scalar {
    #[cfg(test)]
    pub fn from_hex(sk_hex: &str) -> Self {
        let bytes = hex::decode(sk_hex).expect("input must be hex encoded");

        Scalar(Secp256k1Scalar::deserialize(&bytes).expect("input must represent a scalar"))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        match self.0.underlying_ref() {
            Some(secret_key) => secret_key.as_ref(),
            // None represents "zero" scalar in `curv`
            None => &[0; 32],
        }
    }
}

/// Ethereum crypto scheme (as defined by the Key Manager contract)
pub struct EthSigning {}

impl CryptoScheme for EthSigning {
    type Point = Point;
    type Signature = EthSchnorrSignature;

    fn build_signature(z: Scalar, group_commitment: Self::Point) -> Self::Signature {
        EthSchnorrSignature {
            s: *z.as_bytes(),
            r: group_commitment.get_element(),
        }
    }

    /// Assembles and hashes the challenge in the correct order for the KeyManager Contract
    fn build_challenge(
        pubkey: Self::Point,
        nonce_commitment: Self::Point,
        message: &[u8],
    ) -> Scalar {
        use crate::eth::utils::pubkey_to_eth_addr;
        use cf_chains::eth::AggKey;

        let msg_hash: [u8; 32] = message
            .try_into()
            .expect("Should never fail, the `message` argument should always be a valid hash");

        let e = AggKey::from(&pubkey.get_element()).message_challenge(
            &msg_hash,
            &pubkey_to_eth_addr(nonce_commitment.get_element()),
        );

        Scalar::from_bytes(&e)
    }

    fn build_response(
        nonce: <Self::Point as ECPoint>::Scalar,
        private_key: &<Self::Point as ECPoint>::Scalar,
        challenge: <Self::Point as ECPoint>::Scalar,
    ) -> <Self::Point as ECPoint>::Scalar {
        nonce - challenge * private_key
    }
}
