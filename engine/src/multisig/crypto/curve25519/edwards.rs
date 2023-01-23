use serde::{Deserialize, Serialize};

use super::{super::ECPoint, Scalar};

type PK = curve25519_dalek::edwards::EdwardsPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point(PK);

mod point_impls {

	use super::*;

	impl ECPoint for Point {
		type Scalar = Scalar;

		type CompressedPointLength = typenum::U32;

		fn from_scalar(scalar: &Self::Scalar) -> Self {
			Point(curve25519_dalek::constants::ED25519_BASEPOINT_POINT * scalar.0)
		}

		fn as_bytes(&self) -> generic_array::GenericArray<u8, Self::CompressedPointLength> {
			self.0.compress().to_bytes().into()
		}

		fn point_at_infinity() -> Self {
			use curve25519_dalek::traits::Identity;
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
