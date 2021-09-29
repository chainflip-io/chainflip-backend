// We want to re-export certain types here
// to make sure all of our dependencies on
// this module are in one place
mod bitcoin_schnorr;
mod error;

pub use bitcoin_schnorr::{KeyGenBroadcastMessage1, KeyShare, Keys, Parameters};

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
