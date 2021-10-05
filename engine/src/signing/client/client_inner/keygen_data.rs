use serde::{Deserialize, Serialize};

use super::{
    common::BroadcastVerificationMessage,
    keygen_frost::{CoefficientCommitments, ShamirShare, ZKPSignature},
};

macro_rules! derive_impls_for_keygen_data {
    ($variant: ty, $variant_path: path) => {
        derive_impls_for_enum_variants!($variant, $variant_path, KeygenData);
    };
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeygenData {
    Comm1(Comm1),
    Verify2(VerifyComm2),
    SecretShares3(SecretShare3),
    Complaints4(Complaints4),
    VerifyComplaints5(VerifyComplaints5),
}

impl std::fmt::Display for KeygenData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            KeygenData::Comm1(comm1) => comm1.to_string(),
            KeygenData::Verify2(verify2) => verify2.to_string(),
            KeygenData::SecretShares3(share3) => share3.to_string(),
            KeygenData::Complaints4(complaints4) => complaints4.to_string(),
            KeygenData::VerifyComplaints5(verify5) => verify5.to_string(),
        };
        write!(f, "KeygenData({})", inner)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comm1 {
    pub commitments: CoefficientCommitments,
    pub zkp: ZKPSignature,
}

pub type VerifyComm2 = BroadcastVerificationMessage<Comm1>;

// TODO: should this be a simple Scalar with an implicit index?
/// Secret share of our locally generated secret calculated separately
/// for each party as the result of evaluating sharing polynomial (generated
/// during stage 1) at the corresponding signer's index
pub type SecretShare3 = ShamirShare;

/// List of parties blamed for sending invalid secret shares
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Complaints4(pub Vec<usize>);

pub type VerifyComplaints5 = BroadcastVerificationMessage<Complaints4>;

derive_impls_for_keygen_data!(Comm1, KeygenData::Comm1);
derive_impls_for_keygen_data!(VerifyComm2, KeygenData::Verify2);
derive_impls_for_keygen_data!(ShamirShare, KeygenData::SecretShares3);
derive_impls_for_keygen_data!(Complaints4, KeygenData::Complaints4);
derive_impls_for_keygen_data!(VerifyComplaints5, KeygenData::VerifyComplaints5);
