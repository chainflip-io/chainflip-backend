use serde::{Deserialize, Serialize};

use super::super::ECPoint;

type PK = curve25519_dalek::ristretto::RistrettoPoint;

use super::Scalar;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point(PK);

mod point_impls {

	use curve25519_dalek::traits::Identity;

	use super::*;

	impl Point {
		pub fn get_element(&self) -> PK {
			self.0
		}
	}

	impl Ord for Point {
		fn cmp(&self, other: &Self) -> std::cmp::Ordering {
			self.as_bytes().cmp(&other.as_bytes())
		}
	}

	impl PartialOrd for Point {
		fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
			Some(self.cmp(other))
		}
	}

	impl ECPoint for Point {
		type Scalar = Scalar;

		type CompressedPointLength = typenum::U32;

		fn from_scalar(scalar: &Self::Scalar) -> Self {
			Point(curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT * scalar.0)
		}

		fn as_bytes(&self) -> generic_array::GenericArray<u8, Self::CompressedPointLength> {
			self.0.compress().to_bytes().into()
		}

		fn point_at_infinity() -> Self {
			Point(PK::identity())
		}
	}

	derive_point_impls!(Point, Scalar);

	impl std::ops::Add for Point {
		type Output = Self;

		fn add(self, rhs: Self) -> Self::Output {
			Point(self.0 + rhs.0)
		}
	}

	impl std::ops::Sub for Point {
		type Output = Self;

		fn sub(self, rhs: Self) -> Self::Output {
			Point(self.0 - rhs.0)
		}
	}

	impl<B: std::borrow::Borrow<Scalar>> std::ops::Mul<B> for Point {
		type Output = Self;

		fn mul(self, rhs: B) -> Self::Output {
			Point(self.0 * rhs.borrow().0)
		}
	}
}

#[test]
fn sanity_check_point_at_infinity() {
	use super::ECScalar;
	// Sanity check: point at infinity should correspond
	// to "zero" on the elliptic curve
	assert_eq!(Point::point_at_infinity(), Point::from_scalar(&Scalar::zero()));
}
