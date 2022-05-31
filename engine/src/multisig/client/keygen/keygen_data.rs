use std::collections::{BTreeMap, BTreeSet};

use cf_traits::AuthorityCount;
use serde::{Deserialize, Serialize};

use crate::multisig::{client::common::BroadcastVerificationMessage, crypto::ECPoint};

use super::keygen_frost::ShamirShare;

/// Data sent between parties over p2p for a keygen ceremony
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeygenData<P: ECPoint> {
    HashComm1(HashComm1),
    VerifyHashComm2(VerifyHashComm2),
    #[serde(bound = "")] // see https://github.com/serde-rs/serde/issues/1296
    CoeffComm3(CoeffComm3<P>),
    #[serde(bound = "")]
    VerifyCoeffComm4(VerifyCoeffComm4<P>),
    #[serde(bound = "")]
    SecretShares5(SecretShare5<P>),
    Complaints6(Complaints6),
    VerifyComplaints7(VerifyComplaints7),
    #[serde(bound = "")]
    BlameResponse8(BlameResponse8<P>),
    #[serde(bound = "")]
    VerifyBlameResponses9(VerifyBlameResponses9<P>),
}

impl<P: ECPoint> std::fmt::Display for KeygenData<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            KeygenData::HashComm1(inner) => inner.to_string(),
            KeygenData::VerifyHashComm2(inner) => inner.to_string(),
            KeygenData::CoeffComm3(inner) => inner.to_string(),
            KeygenData::VerifyCoeffComm4(inner) => inner.to_string(),
            KeygenData::SecretShares5(inner) => inner.to_string(),
            KeygenData::Complaints6(inner) => inner.to_string(),
            KeygenData::VerifyComplaints7(inner) => inner.to_string(),
            KeygenData::BlameResponse8(inner) => inner.to_string(),
            KeygenData::VerifyBlameResponses9(inner) => inner.to_string(),
        };
        write!(f, "KeygenData({})", inner)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashComm1(pub sp_core::H256);

pub type VerifyHashComm2 = BroadcastVerificationMessage<HashComm1>;

pub type CoeffComm3<P> = super::keygen_frost::DKGUnverifiedCommitment<P>;

pub type VerifyCoeffComm4<P> = BroadcastVerificationMessage<CoeffComm3<P>>;

/// Secret share of our locally generated secret calculated separately
/// for each party as the result of evaluating sharing polynomial (generated
/// during stage 1) at the corresponding signer's index
pub type SecretShare5<P> = ShamirShare<P>;

/// List of parties blamed for sending invalid secret shares
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Complaints6(pub BTreeSet<AuthorityCount>);

pub type VerifyComplaints7 = BroadcastVerificationMessage<Complaints6>;

/// For each party blaming a node, it responds with the corresponding (valid)
/// secret share. Unlike secret shares sent at the earlier stage, these shares
/// are verifiably broadcast, so sending an invalid share would result in the
/// node being slashed. Although the shares are meant to be secret, it is safe
/// to reveal/broadcast some them at this stage: a node's long-term secret can
/// only be recovered by collecting shares from all (N-1) nodes, which would
/// require collusion of N-1 nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlameResponse8<P: ECPoint>(
    #[serde(bound = "")] pub BTreeMap<AuthorityCount, ShamirShare<P>>,
);

pub type VerifyBlameResponses9<P> = BroadcastVerificationMessage<BlameResponse8<P>>;

derive_impls_for_enum_variants!(impl<P: ECPoint> for HashComm1, KeygenData::HashComm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyHashComm2, KeygenData::VerifyHashComm2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for CoeffComm3<P>, KeygenData::CoeffComm3, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyCoeffComm4<P>, KeygenData::VerifyCoeffComm4, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for SecretShare5<P>, KeygenData::SecretShares5, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Complaints6, KeygenData::Complaints6, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComplaints7, KeygenData::VerifyComplaints7, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for BlameResponse8<P>, KeygenData::BlameResponse8, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyBlameResponses9<P>, KeygenData::VerifyBlameResponses9, KeygenData<P>);

derive_display_as_type_name!(HashComm1);
derive_display_as_type_name!(VerifyHashComm2);
derive_display_as_type_name!(CoeffComm3<P: ECPoint>);
derive_display_as_type_name!(VerifyCoeffComm4<P: ECPoint>);
derive_display_as_type_name!(SecretShare5<P: ECPoint>);
derive_display_as_type_name!(Complaints6);
derive_display_as_type_name!(VerifyComplaints7);
derive_display_as_type_name!(BlameResponse8<P: ECPoint>);
derive_display_as_type_name!(VerifyBlameResponses9<P: ECPoint>);
