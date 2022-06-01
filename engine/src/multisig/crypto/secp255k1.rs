use generic_array::GenericArray;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{ECPoint, ECScalar, Rng};
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

#[derive(Clone, Debug, PartialEq)]
pub struct Scalar(Secp256k1Scalar);

mod point_impls {

    use super::*;

    derive_point_impls!(Point, Scalar);

    impl std::ops::Mul<&Scalar> for Point {
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

    impl std::ops::Sub for Point {
        type Output = Point;

        fn sub(self, rhs: Self) -> Self::Output {
            Point(self.0.sub_point(&rhs.0))
        }
    }

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

        fn point_at_infinity() -> Self {
            Self(Secp256k1Point::zero())
        }

        fn is_compatible(&self) -> bool {
            // Check if the public key's x coordinate is smaller than "half secp256k1's order",
            // which is a requirement imposed by the Key Manager contract
            let pk = self.get_element();
            let pubkey = cf_chains::eth::AggKey::from_pubkey_compressed(pk.serialize());

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
}

mod scalar_impls {

    use super::*;

    derive_scalar_impls!(Scalar);

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

        fn zero() -> Self {
            Scalar(Secp256k1Scalar::zero())
        }

        fn invert(&self) -> Option<Self> {
            self.0.invert().map(Scalar)
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

    impl From<u32> for Scalar {
        fn from(x: u32) -> Self {
            Scalar(CurvECScalar::from_bigint(&BigInt::from(x)))
        }
    }

    impl std::ops::Sub for &Scalar {
        type Output = Scalar;

        fn sub(self, rhs: Self) -> Self::Output {
            Scalar(self.0.sub(&rhs.0))
        }
    }

    impl std::ops::Mul for &Scalar {
        type Output = Scalar;

        fn mul(self, rhs: Self) -> Self::Output {
            Scalar(self.0.mul(&rhs.0))
        }
    }

    impl std::ops::Add for &Scalar {
        type Output = Scalar;

        fn add(self, rhs: Self) -> Self::Output {
            Scalar(self.0.add(&rhs.0))
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
}

#[test]
fn sanity_check_point_at_infinity() {
    // Sanity check: point at infinity should correspond
    // to "zero" on the elliptic curve
    assert_eq!(
        Point::point_at_infinity(),
        Point::from_scalar(&Scalar::zero())
    );
}
