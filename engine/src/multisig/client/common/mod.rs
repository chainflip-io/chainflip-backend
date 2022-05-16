pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use broadcast_verification::BroadcastVerificationMessage;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::multisig::crypto::{KeyShare, Point};

use super::{utils::PartyIdxMapping, ThresholdParameters};

use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResult {
    pub key_share: KeyShare,
    pub party_public_keys: Vec<Point>,
}

impl KeygenResult {
    pub fn get_public_key(&self) -> Point {
        self.key_share.y
    }

    /// Gets the serialized compressed public key (33 bytes - 32 bytes + a y parity byte)
    pub fn get_public_key_bytes(&self) -> Vec<u8> {
        use crate::multisig::crypto::ECPoint;
        self.key_share.y.0.serialize_compressed().as_ref().into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResultInfo {
    pub key: Arc<KeygenResult>,
    pub validator_map: Arc<PartyIdxMapping>,
    pub params: ThresholdParameters,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyFailureReason<T> {
    #[error("Request Ignored (Duplicate Ceremony Id)")]
    DuplicateCeremonyId,
    #[error("Expired before being authorized")]
    ExpiredBeforeBeingAuthorized,
    #[error("Request Ignored (Invalid Participants)")]
    InvalidParticipants,
    #[error("Request Ignored (Ceremony Id already used)")]
    CeremonyIdAlreadyUsed,
    #[error("Broadcast Failure ({0}) during {1} stage")]
    BroadcastFailure(BroadcastFailureReason, BroadcastStageName),
    #[error("{0}")]
    Other(T),
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigningFailureReason {
    #[error("Invalid Sig Share")]
    InvalidSigShare,
    #[error("Request Ignored (Not Enough Signers)")]
    NotEnoughSigners,
    #[error("Request Ignored (Unknown Key)")]
    UnknownKey,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenFailureReason {
    #[error("Invalid Commitment")]
    InvalidCommitment,
    #[error("Invalid secret share in a blame response")]
    InvalidBlameResponse,
    #[error("The key is not contract compatible")]
    NotContractCompatible,
    #[error("Invalid Complaint")]
    InvalidComplaint,
    #[error("High Degree Coefficient Zero")]
    HighDegreeCoefficientZero,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastFailureReason {
    /// Enough missing messages from broadcast + verification to stop consensus
    #[error("Insufficient Messages")]
    InsufficientMessages,
    /// Not enough broadcast verification messages received to continue verification
    #[error("Insufficient Verification Messages")]
    InsufficientVerificationMessages,
    /// Consensus could not be reached for one or more parties due to differing values
    #[error("Inconsistency")]
    Inconsistency,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastStageName {
    #[error("Initial Commitments")]
    InitialCommitments,
    #[error("Local Signatures")]
    LocalSignatures,
    #[error("Hash Commitments")]
    HashCommitments,
    #[error("Complaints")]
    Complaints,
    #[error("Blame Responses")]
    BlameResponses,
}

// impl Display for BroadcastStageName {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             BroadcastStageName::InitialCommitments => {
//                 write!(f, "Initial Commitments")
//             }
//             BroadcastStageName::LocalSignatures => {
//                 write!(f, "Local Signatures")
//             }
//             BroadcastStageName::HashCommitments => {
//                 write!(f, "Hash Commitments")
//             }
//             BroadcastStageName::Complaints => {
//                 write!(f, "Complaints")
//             }
//             BroadcastStageName::BlameResponses => {
//                 write!(f, "Blame Responses")
//             }
//         }
//     }
// }

// impl<T> Display for CeremonyFailureReason<T>
// where
//     T: Display,
// {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             CeremonyFailureReason::DuplicateCeremonyId => {
//                 write!(f, "Duplicate Ceremony Id")
//             }
//             CeremonyFailureReason::ExpiredBeforeBeingAuthorized => {
//                 write!(f, "Ceremony expired before being authorized")
//             }
//             CeremonyFailureReason::Other(reason) => {
//                 write!(f, "{}", reason)
//             }
//         }
//     }
// }

// impl Display for KeygenFailureReason {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let inner = match self {
//             KeygenFailureReason::RequestIgnored(reason) => {
//                 format!("Request Ignored ({:?})", reason)
//             }
//             KeygenFailureReason::BroadcastFailure(reason, stage_name) => {
//                 format!(
//                     "Broadcast failure ({:?}) during {} stage",
//                     reason, stage_name
//                 )
//             }
//             KeygenFailureReason::NotContractCompatible => {
//                 "The key is not contract compatible".to_string()
//             }
//             KeygenFailureReason::InvalidBlameResponse => {
//                 "Invalid secret share in a blame response".to_string()
//             }
//             _ => format!("{:?}", self),
//         };
//         write!(f, "Keygen Failure: {}", inner)
//     }
// }

// impl Display for SigningFailureReason {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let inner = match self {
//             SigningFailureReason::RequestIgnored(reason) => {
//                 format!("Request Ignored ({:?})", reason)
//             }
//             SigningFailureReason::BroadcastFailure(reason, stage_name) => {
//                 format!("Broadcast failure ({:?}) during {:?}", reason, stage_name)
//             }
//             SigningFailureReason::InvalidSigShare => "Failed to aggregate signature".to_string(),
//         };
//         write!(f, "Signing Failure: {}", inner)
//     }
// }
