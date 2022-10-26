use super::{ECPoint, ECScalar, Rng};
use num_bigint::BigUint;
use secp256k1::constants::{CURVE_ORDER, SECRET_KEY_SIZE};
use serde::{Deserialize, Serialize};

type SK = secp256k1::SecretKey;
type PK = secp256k1::PublicKey;

// Wrapping in `Option` to make it easier to keep track
// of "zero" scalars which often need special treatment
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scalar(Option<SK>);

// None if it is a "point at infinity"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point(Option<PK>);

const GENERATOR_COMPRESSED: [u8; 33] = [
	0x02, 0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87, 0x0B,
	0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B, 0x16, 0xF8, 0x17,
	0x98,
];

lazy_static::lazy_static! {
	static ref GENERATOR: Point = Point(Some(PK::from_slice(&GENERATOR_COMPRESSED).unwrap()));
	static ref GROUP_ORDER_BIG_UINT: BigUint = BigUint::from_bytes_be(&CURVE_ORDER);
}

mod point_impls {

	use super::*;

	const POINT_AT_INFINITY_COMPRESSED: [u8; 33] = [0; 33];
	const POINT_AT_INFINITY_UNCOMPRESSED: [u8; 65] = [0; 65];

	derive_point_impls!(Point, Scalar);

	impl<B: std::borrow::Borrow<Scalar>> std::ops::Mul<B> for Point {
		type Output = Self;

		fn mul(self, scalar: B) -> Self::Output {
			let inner = match (self.0, scalar.borrow().0) {
				(None, _) | (_, None) => {
					// multiplication by 0 creates a "point at infinity"
					None
				},
				(Some(mut point), Some(scalar)) => {
					point
						.mul_assign(secp256k1::SECP256K1, scalar.as_ref())
						.expect("scalar must be valid and non-zero");
					Some(point)
				},
			};

			Point(inner)
		}
	}

	impl std::ops::Add for Point {
		type Output = Self;

		fn add(self, rhs: Self) -> Self::Output {
			let inner = match (self.0, rhs.0) {
				(None, rhs) => rhs,
				(lhs, None) => lhs,
				(Some(lhs), Some(rhs)) => {
					// this can only fail if the result is
					// a point at infinity which we represent
					// with `None`
					lhs.combine(&rhs).ok()
				},
			};
			Point(inner)
		}
	}

	impl std::ops::Sub for Point {
		type Output = Self;

		// Silence clippy as addition is here by design
		// (note that we negate the right operand first)
		#[allow(clippy::suspicious_arithmetic_impl)]
		fn sub(self, mut rhs: Self) -> Self::Output {
			// Only negate if non-zero
			if let Some(rhs) = rhs.0.as_mut() {
				rhs.negate_assign(secp256k1::SECP256K1)
			}

			self + rhs
		}
	}

	impl ECPoint for Point {
		type Scalar = Scalar;
		type CompressedPointLength = typenum::U33;

		fn from_scalar(scalar: &Self::Scalar) -> Self {
			*Self::generator() * scalar
		}

		fn as_bytes(&self) -> generic_array::GenericArray<u8, Self::CompressedPointLength> {
			match self.0 {
				Some(pk) => pk.serialize(),
				None => POINT_AT_INFINITY_COMPRESSED,
			}
			.into()
		}

		fn point_at_infinity() -> Self {
			Point(None)
		}
	}

	impl Serialize for Point {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: serde::Serializer,
		{
			serializer.serialize_bytes(&self.as_bytes())
		}
	}

	impl<'de> Deserialize<'de> for Point {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: serde::Deserializer<'de>,
		{
			let bytes = Vec::deserialize(deserializer)?;

			// Check both compressed and uncompressed
			// representations of zero (even though we
			// only use compressed)
			if bytes == POINT_AT_INFINITY_COMPRESSED || bytes == POINT_AT_INFINITY_UNCOMPRESSED {
				Ok(Point::point_at_infinity())
			} else {
				PK::from_slice(&bytes)
					.map(|pk| Point(Some(pk)))
					.map_err(serde::de::Error::custom)
			}
		}
	}

	impl Point {
		fn generator() -> &'static Point {
			&GENERATOR
		}

		pub fn get_element(&self) -> secp256k1::PublicKey {
			// We can be reasonably sure that the point is
			// valid (i.e. not a point at infinity) as the
			// method is only called on aggregate values and
			// cannot be controlled by any single party (the
			// chance of getting an invalid point by chance
			// is negligible)
			self.0.expect("unexpected point at infinity")
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
		/// Expects `x` to be within the group, i.e.
		/// it is smaller than the group's order
		fn from_reduced_bigint(x: &BigUint) -> Self {
			use num_traits::identities::Zero;

			assert!(x < &GROUP_ORDER_BIG_UINT, "x not within the group");

			if x.is_zero() {
				Scalar(None)
			} else {
				let x_bytes = x.to_bytes_be();
				let mut array = [0u8; SECRET_KEY_SIZE];
				array[SECRET_KEY_SIZE - x_bytes.len()..].copy_from_slice(&x_bytes);

				// Safe because `x` is within the group
				// and `array` is correct size by construction
				Scalar(Some(SK::from_slice(&array).unwrap()))
			}
		}

		pub fn as_bytes(&self) -> &[u8; SECRET_KEY_SIZE] {
			match self.0.as_ref() {
				Some(sk) => sk.as_ref(),
				None => &ZERO_SCALAR_BYTES,
			}
		}
	}

	impl ECScalar for Scalar {
		fn random(rng: &mut Rng) -> Self {
			let sk = SK::new(rng);
			// The key is guaranteed to be non-zero by
			// the implementation of SK::new
			Scalar(Some(sk))
		}

		fn from_bytes_mod_order(x: &[u8; 32]) -> Self {
			// reduce `x` to make it a valid element in the group
			let x = {
				let mut x = BigUint::from_bytes_be(x);

				// Because the source is only 32 bytes, we know that
				// it must be smaller than twice secp256k1's order,
				// so a single subtraction is sufficient here
				if x >= *GROUP_ORDER_BIG_UINT {
					x -= &*GROUP_ORDER_BIG_UINT;
				}
				x
			};

			Self::from_reduced_bigint(&x)
		}

		fn zero() -> Self {
			Scalar(None)
		}

		// Note that we don't need this to be constant-time as we
		// only invert public values.
		fn invert(&self) -> Option<Self> {
			self.0.map(|x| {
				let x = BigUint::from_bytes_be(x.as_ref());

				let order = BigUint::from_bytes_be(&CURVE_ORDER);

				// Modular multiplicative inverse is equivalent to raising
				// to the power of `order - 2` if the order is prime (using Euler's theorem; also
				// see libsecp256k1 which uses a somewhat similar implementation:
				// https://docs.rs/libsecp256k1-core/0.3.0/src/libsecp256k1_core/field.rs.html#1546)
				let inverse = x.modpow(&(&order - 2u32), &order);

				Self::from_reduced_bigint(&inverse)
			})
		}
	}

	impl zeroize::Zeroize for Scalar {
		fn zeroize(&mut self) {
			use core::sync::atomic;
			unsafe { std::ptr::write_volatile(self, Scalar::zero()) };
			atomic::compiler_fence(atomic::Ordering::SeqCst);
		}
	}

	impl From<u32> for Scalar {
		fn from(x: u32) -> Self {
			if x == 0 {
				Scalar(None)
			} else {
				let mut array = [0u8; 32];
				array[28..].copy_from_slice(&x.to_be_bytes());

				// Since `x` is u32, we know it to be within
				// the curve order, and the slice is 32 bytes
				// by construction, so this cannot fail
				Scalar(Some(SK::from_slice(&array).unwrap()))
			}
		}
	}

	impl std::ops::Sub for &Scalar {
		type Output = Scalar;

		// Silence clippy as addition is here by design
		// (note that we negate the right operand first)
		#[allow(clippy::suspicious_arithmetic_impl)]
		fn sub(self, rhs: Self) -> Self::Output {
			// according to https://github.com/bitcoin-core/secp256k1/blob/44c2452fd387f7ca604ab42d73746e7d3a44d8a2/include/secp256k1.h#L649
			// `negate_assign` expects a valid non-zero scalar

			match rhs.0 {
				None => self.clone(),
				Some(mut x) => {
					// it is safe to negate non-zero Scalar
					x.negate_assign();

					self + &Scalar(Some(x))
				},
			}
		}
	}

	impl std::ops::Mul for &Scalar {
		type Output = Scalar;

		fn mul(self, rhs: Self) -> Self::Output {
			let inner = match (self.0, rhs.0) {
				(None, _) | (_, None) => None,
				(Some(mut lhs), Some(rhs)) => {
					lhs.mul_assign(rhs.as_ref()).expect("can't fail if both operands are valid");
					// implementation of mul_assign never returns
					// a zero scalar
					Some(lhs)
				},
			};
			Scalar(inner)
		}
	}

	impl std::ops::Add for &Scalar {
		type Output = Scalar;

		fn add(self, rhs: Self) -> Self::Output {
			let inner = match (self.0, rhs.0) {
				(None, rhs) => rhs,
				(lhs, None) => lhs,
				(Some(mut lhs), Some(rhs)) => {
					// Both lhs and rhs are considered "valid" (i.e.
					// non-zero and belong to the group). Further,
					// the addition is done modulo group order, so
					// this function can only fail if the result
					// itself is zero
					lhs.add_assign(rhs.as_ref()).ok().map(|_| lhs)
				},
			};

			Scalar(inner)
		}
	}

	const ZERO_SCALAR_BYTES: [u8; 32] = [0; 32];

	impl Serialize for Scalar {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: serde::Serializer,
		{
			match self.0 {
				Some(x) => serializer.serialize_bytes(x.as_ref()),
				None => serializer.serialize_bytes(&ZERO_SCALAR_BYTES),
			}
		}
	}

	impl<'de> Deserialize<'de> for Scalar {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: serde::Deserializer<'de>,
		{
			let bytes = Vec::deserialize(deserializer)?;
			if bytes == ZERO_SCALAR_BYTES {
				Ok(Scalar::zero())
			} else {
				SK::from_slice(&bytes)
					.map(|x| Scalar(Some(x)))
					.map_err(serde::de::Error::custom)
			}
		}
	}

	#[cfg(test)]
	impl Scalar {
		pub fn from_hex(sk_hex: &str) -> Self {
			let bytes = hex::decode(sk_hex).expect("input must be hex encoded");
			// `from_slice` never returns 0
			Scalar(Some(SK::from_slice(&bytes).expect("invalid scalar")))
		}
	}
}
