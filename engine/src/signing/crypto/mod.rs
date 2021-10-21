// We want to re-export certain types here
// to make sure all of our dependencies on
// this module are in one place
pub use curv::{
    arithmetic::traits::Converter as BigIntConverter,
    cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS,
    elliptic::curves::{
        secp256_k1::{FE as Scalar, GE as Point},
        traits::{ECPoint, ECScalar},
    },
    BigInt,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyShare {
    pub y: Point,
    pub x_i: Scalar,
}

/// Allows us to extend a third party type
pub trait ScalarExt {
    type Scalar;

    fn from_usize(a: usize) -> Scalar;
}

impl ScalarExt for Scalar {
    type Scalar = Scalar;

    fn from_usize(a: usize) -> Self {
        ECScalar::from(&BigInt::from(a as u32))
    }
}
