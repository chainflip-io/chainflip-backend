// We want to re-export certain types here
// to make sure all of our dependencies on
// this module are in one place
mod error;
// mod schnorr;

// pub use schnorr::{KeyGenBroadcastMessage1, KeyShare, Keys, Parameters};

pub use error::{InvalidKey, InvalidSS, InvalidSig};

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

pub trait ScalarExt {
    type Scalar;

    fn one() -> Scalar;

    fn from_usize(a: usize) -> Scalar;

    fn sub_scalar(self, a: Self) -> Self;
}

impl ScalarExt for Scalar {
    type Scalar = Scalar;

    fn one() -> Self {
        Self::from_usize(1usize)
    }

    fn from_usize(a: usize) -> Self {
        ECScalar::from(&BigInt::from(a as u32))
    }

    fn sub_scalar(self, a: Self) -> Self {
        self.sub(&a.get_element())
    }
}
