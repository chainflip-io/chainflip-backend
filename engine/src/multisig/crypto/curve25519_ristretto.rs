use serde::{Deserialize, Serialize};

use super::{ECPoint, ECScalar};

type SK = curve25519_dalek::scalar::Scalar;
type PK = curve25519_dalek::ristretto::RistrettoPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point(PK);

// TODO: this scalar is shared between ristretto and edwards,
// so it should be moved outside
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scalar(pub(super) SK);

mod point_impls {

	use curve25519_dalek::traits::Identity;

	use super::*;

	impl Point {
		pub fn get_element(&self) -> PK {
			self.0
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

mod scalar_impls {

	use zeroize::Zeroize;

	use super::*;

	impl ECScalar for Scalar {
		fn random(rng: &mut crate::multisig::crypto::Rng) -> Self {
			use rand_legacy::RngCore;

			// Instead of calling SK::random() directly, we copy its
			// implementation so we can use our own (version of) Rng
			// TODO: might as well use a more recent version of Rng
			// and apply this trick where an older version is expected
			// (instead of the other way around)
			let mut scalar_bytes = [0u8; 64];
			rng.fill_bytes(&mut scalar_bytes);
			Scalar(SK::from_bytes_mod_order_wide(&scalar_bytes))
		}

		fn from_bytes_mod_order(x: &[u8; 32]) -> Self {
			Scalar(SK::from_bytes_mod_order(*x))
		}

		fn zero() -> Self {
			Scalar(SK::zero())
		}

		fn invert(&self) -> Option<Self> {
			if self.0 != SK::zero() {
				Some(Scalar(self.0.invert()))
			} else {
				None
			}
		}
	}

	impl From<u32> for Scalar {
		fn from(x: u32) -> Self {
			Scalar(SK::from(x))
		}
	}

	impl From<SK> for Scalar {
		fn from(sk: SK) -> Self {
			Scalar(sk)
		}
	}

	impl Scalar {
		pub fn to_bytes(&self) -> [u8; 32] {
			self.0.to_bytes()
		}
	}

	derive_scalar_impls!(Scalar);

	impl Zeroize for Scalar {
		fn zeroize(&mut self) {
			self.0.zeroize();
		}
	}

	impl std::ops::Add for &Scalar {
		type Output = Scalar;

		fn add(self, rhs: Self) -> Self::Output {
			Scalar(self.0 + rhs.0)
		}
	}

	impl std::ops::Sub for &Scalar {
		type Output = Scalar;

		fn sub(self, rhs: Self) -> Self::Output {
			Scalar(self.0 - rhs.0)
		}
	}

	impl std::ops::Mul for &Scalar {
		type Output = Scalar;

		fn mul(self, rhs: Self) -> Self::Output {
			Scalar(self.0 * rhs.0)
		}
	}
}

#[test]
fn sanity_check_point_at_infinity() {
	// Sanity check: point at infinity should correspond
	// to "zero" on the elliptic curve
	assert_eq!(Point::point_at_infinity(), Point::from_scalar(&Scalar::zero()));
}
