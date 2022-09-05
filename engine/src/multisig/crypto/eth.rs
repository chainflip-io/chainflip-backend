use crate::multisig::crypto::ECScalar;

use super::{ChainTag, CryptoScheme, ECPoint};

// NOTE: for now, we re-export these to make it
// clear that these a the primitives used by ethereum.
// TODO: we probably want to change the "clients" to
// solely use "CryptoScheme" as generic parameter instead.
pub use super::secp256k1::{Point, Scalar};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthSchnorrSignature {
    /// Scalar component
    pub s: [u8; 32],
    /// Point component (commitment)
    pub r: secp256k1::PublicKey,
}

impl From<EthSchnorrSignature> for cf_chains::eth::SchnorrVerificationComponents {
    fn from(cfe_sig: EthSchnorrSignature) -> Self {
        use crate::eth::utils::pubkey_to_eth_addr;

        Self {
            s: cfe_sig.s,
            k_times_g_address: pubkey_to_eth_addr(cfe_sig.r),
        }
    }
}

/// Ethereum crypto scheme (as defined by the Key Manager contract)
pub struct EthSigning {}

impl CryptoScheme for EthSigning {
    type Point = Point;
    type Signature = EthSchnorrSignature;

    const NAME: &'static str = "Ethereum";
    const CHAIN_TAG: ChainTag = ChainTag::Ethereum;

    fn build_signature(z: Scalar, group_commitment: Self::Point) -> Self::Signature {
        EthSchnorrSignature {
            s: *z.as_bytes(),
            r: group_commitment.get_element(),
        }
    }

    /// Assembles and hashes the challenge in the correct order for the KeyManager Contract
    fn build_challenge(
        pubkey: Self::Point,
        nonce_commitment: Self::Point,
        msg_hash: &[u8; 32],
    ) -> Scalar {
        use crate::eth::utils::pubkey_to_eth_addr;
        use cf_chains::eth::AggKey;

        let e = AggKey::from_pubkey_compressed(pubkey.get_element().serialize()).message_challenge(
            msg_hash,
            &pubkey_to_eth_addr(nonce_commitment.get_element()),
        );

        Scalar::from_bytes_mod_order(&e)
    }

    fn build_response(
        nonce: <Self::Point as ECPoint>::Scalar,
        private_key: &<Self::Point as ECPoint>::Scalar,
        challenge: <Self::Point as ECPoint>::Scalar,
    ) -> <Self::Point as ECPoint>::Scalar {
        nonce - challenge * private_key
    }
}
