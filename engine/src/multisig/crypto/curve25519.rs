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
