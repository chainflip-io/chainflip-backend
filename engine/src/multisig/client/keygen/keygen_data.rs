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
    Comm1(Comm1<P>),
    #[serde(bound = "")]
    Verify2(VerifyComm2<P>),
    #[serde(bound = "")]
    SecretShares3(SecretShare3<P>),
    Complaints4(Complaints4),
    VerifyComplaints5(VerifyComplaints5),
    #[serde(bound = "")]
    BlameResponse6(BlameResponse6<P>),
    #[serde(bound = "")]
    VerifyBlameResponses7(VerifyBlameResponses7<P>),
}

impl<P: ECPoint> std::fmt::Display for KeygenData<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            KeygenData::HashComm1(hash_comm1) => hash_comm1.to_string(),
            KeygenData::VerifyHashComm2(verify2) => verify2.to_string(),
            KeygenData::Comm1(comm1) => comm1.to_string(),
            KeygenData::Verify2(verify2) => verify2.to_string(),
            KeygenData::SecretShares3(share3) => share3.to_string(),
            KeygenData::Complaints4(complaints4) => complaints4.to_string(),
            KeygenData::VerifyComplaints5(verify5) => verify5.to_string(),
            KeygenData::BlameResponse6(blame_response) => blame_response.to_string(),
            KeygenData::VerifyBlameResponses7(verify7) => verify7.to_string(),
        };
        write!(f, "KeygenData({})", inner)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashComm1(pub sp_core::H256);

pub type VerifyHashComm2 = BroadcastVerificationMessage<HashComm1>;

pub type Comm1<P> = super::keygen_frost::DKGUnverifiedCommitment<P>;

pub type VerifyComm2<P> = BroadcastVerificationMessage<Comm1<P>>;

/// Secret share of our locally generated secret calculated separately
/// for each party as the result of evaluating sharing polynomial (generated
/// during stage 1) at the corresponding signer's index
pub type SecretShare3<P> = ShamirShare<P>;

/// List of parties blamed for sending invalid secret shares
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Complaints4(pub BTreeSet<AuthorityCount>);

pub type VerifyComplaints5 = BroadcastVerificationMessage<Complaints4>;

/// For each party blaming a node, it responds with the corresponding (valid)
/// secret share. Unlike secret shares sent at the earlier stage, these shares
/// are verifiably broadcast, so sending an invalid share would result in the
/// node being slashed. Although the shares are meant to be secret, it is safe
/// to reveal/broadcast some them at this stage: a node's long-term secret can
/// only be recovered by collecting shares from all (N-1) nodes, which would
/// require collusion of N-1 nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlameResponse6<P: ECPoint>(
    #[serde(bound = "")] pub BTreeMap<AuthorityCount, ShamirShare<P>>,
);

pub type VerifyBlameResponses7<P> = BroadcastVerificationMessage<BlameResponse6<P>>;

derive_impls_for_enum_variants!(impl<P: ECPoint> for HashComm1, KeygenData::HashComm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyHashComm2, KeygenData::VerifyHashComm2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Comm1<P>, KeygenData::Comm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComm2<P>, KeygenData::Verify2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for ShamirShare<P>, KeygenData::SecretShares3, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Complaints4, KeygenData::Complaints4, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComplaints5, KeygenData::VerifyComplaints5, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for BlameResponse6<P>, KeygenData::BlameResponse6, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyBlameResponses7<P>, KeygenData::VerifyBlameResponses7, KeygenData<P>);

derive_display_as_type_name!(HashComm1);
derive_display_as_type_name!(VerifyHashComm2);
derive_display_as_type_name!(ShamirShare<P: ECPoint>);
derive_display_as_type_name!(Complaints4);
derive_display_as_type_name!(VerifyComplaints5);
derive_display_as_type_name!(BlameResponse6<P: ECPoint>);
derive_display_as_type_name!(VerifyBlameResponses7<P: ECPoint>);

derive_display_as_type_name!(Comm1<P: ECPoint>);
derive_display_as_type_name!(VerifyComm2<P: ECPoint>);
