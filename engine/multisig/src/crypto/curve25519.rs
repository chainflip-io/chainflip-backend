pub mod edwards;
pub mod ristretto;

use serde::{Deserialize, Serialize};

use super::ECScalar;

type SK = curve25519_dalek::scalar::Scalar;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scalar(pub(super) SK);

mod scalar_impls {

	use zeroize::Zeroize;

	use super::*;

	impl Ord for Scalar {
		fn cmp(&self, other: &Self) -> std::cmp::Ordering {
			self.0.as_bytes().cmp(other.0.as_bytes())
		}
	}

	impl PartialOrd for Scalar {
		fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
			Some(self.cmp(other))
		}
	}

	impl ECScalar for Scalar {
		fn random(rng: &mut crate::crypto::Rng) -> Self {
			use rand::RngCore;

			// Instead of calling SK::random() directly, we copy its
			// implementation so we can use our own (version of) Rng
			let mut scalar_bytes = [0u8; 64];
			rng.fill_bytes(&mut scalar_bytes);
			Scalar(SK::from_bytes_mod_order_wide(&scalar_bytes))
		}

		fn from_bytes_mod_order(x: &[u8; 32]) -> Self {
			Scalar(SK::from_bytes_mod_order(*x))
		}

		fn zero() -> Self {
			Scalar(SK::ZERO)
		}

		fn invert(&self) -> Option<Self> {
			if self.0 != SK::ZERO {
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
fn ensure_serialization_is_consistent() {
	use super::ECPoint;

	// Scalar is 32 bytes
	let scalar: Scalar = bincode::deserialize(&[
		22, 33, 188, 127, 243, 114, 222, 165, 177, 158, 212, 131, 122, 34, 112, 164, 230, 48, 112,
		90, 14, 78, 91, 42, 120, 206, 28, 215, 160, 190, 21, 0,
	])
	.unwrap();

	// Test Edwards point
	{
		let point = edwards::Point::from_scalar(&scalar);

		// Point is 32 bytes
		let expected_point_bytes = [
			105, 113, 52, 248, 81, 218, 185, 180, 25, 70, 146, 24, 178, 179, 239, 247, 37, 98, 90,
			230, 133, 204, 122, 162, 0, 84, 28, 213, 50, 135, 230, 235,
		];

		assert_eq!(bincode::serialize(&point).unwrap(), expected_point_bytes);
	}

	// Test Ristretto point
	{
		let point = ristretto::Point::from_scalar(&scalar);

		// Point is 32 bytes
		let expected_point_bytes = [
			46, 177, 159, 111, 170, 191, 255, 194, 205, 23, 199, 98, 188, 141, 12, 36, 188, 225,
			13, 218, 203, 150, 50, 216, 195, 73, 245, 243, 5, 221, 23, 118,
		];

		assert_eq!(bincode::serialize(&point).unwrap(), expected_point_bytes);
	}
}
