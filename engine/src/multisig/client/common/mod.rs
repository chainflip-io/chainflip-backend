pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use broadcast_verification::BroadcastVerificationMessage;

use std::{fmt::Display, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::multisig::crypto::{KeyShare, Point};

use super::{utils::PartyIdxMapping, ThresholdParameters};

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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyFailureReason {
    KeygenFailure(KeygenFailureReason),
    SigningFailure(SigningFailureReason),
    DuplicateCeremonyId,
    ExpiredBeforeBeingAuthorized,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigningFailureReason {
    RequestIgnored(SigningRequestIgnoredReason),
    BroadcastFailure(BroadcastFailureReason, BroadcastStageName),
    InvalidSigShare,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenFailureReason {
    RequestIgnored(KeygenRequestIgnoredReason),
    BroadcastFailure(BroadcastFailureReason, BroadcastStageName),
    InvalidCommitment,
    InvalidBlameResponse,
    NotContractCompatible,
    InvalidComplaint,
    HighDegreeCoefficientZero,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigningRequestIgnoredReason {
    NotEnoughSigners,
    UnknownKey,
    InvalidParticipants,
    CeremonyIdAlreadyUsed,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenRequestIgnoredReason {
    CeremonyIdAlreadyUsed,
    InvalidParticipants,
}

#[derive(PartialEq, Debug, PartialOrd, Eq, Ord)]
pub enum BroadcastFailureReason {
    /// Enough missing messages from broadcast + verification to stop consensus
    InsufficientMessages,
    /// Not enough broadcast verification messages received to continue verification
    InsufficientVerificationMessages,
    /// Consensus could not be reached for one or more parties due to differing values
    Inconsistency,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastStageName {
    InitialCommitments,
    LocalSignatures,
    HashCommitments,
    Complaints,
    BlameResponses,
}

impl Display for BroadcastStageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BroadcastStageName::InitialCommitments => {
                write!(f, "Initial Commitments")
            }
            BroadcastStageName::LocalSignatures => {
                write!(f, "Local Signatures")
            }
            BroadcastStageName::HashCommitments => {
                write!(f, "Hash Commitments")
            }
            BroadcastStageName::Complaints => {
                write!(f, "Complaints")
            }
            BroadcastStageName::BlameResponses => {
                write!(f, "Blame Responses")
            }
        }
    }
}

/// Display the ceremony failure reason as a readable string
impl Display for CeremonyFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CeremonyFailureReason::KeygenFailure(keygen_failure_reason) => {
                let inner = match keygen_failure_reason {
                    KeygenFailureReason::RequestIgnored(reason) => {
                        format!("Request Ignored ({:?})", reason)
                    }
                    KeygenFailureReason::BroadcastFailure(reason, stage_name) => {
                        format!(
                            "Broadcast failure ({:?}) during {} stage",
                            reason, stage_name
                        )
                    }
                    KeygenFailureReason::NotContractCompatible => {
                        "The key is not contract compatible".to_string()
                    }
                    KeygenFailureReason::InvalidBlameResponse => {
                        "Invalid secret share in a blame response".to_string()
                    }
                    _ => format!("{:?}", keygen_failure_reason),
                };
                write!(f, "Keygen Failure: {}", inner)
            }
            CeremonyFailureReason::SigningFailure(signing_failure_reason) => {
                let inner = match signing_failure_reason {
                    SigningFailureReason::RequestIgnored(reason) => {
                        format!("Request Ignored ({:?})", reason)
                    }
                    SigningFailureReason::BroadcastFailure(reason, stage_name) => {
                        format!("Broadcast failure ({:?}) during {:?}", reason, stage_name)
                    }
                    SigningFailureReason::InvalidSigShare => {
                        "Failed to aggregate signature".to_string()
                    }
                };
                write!(f, "Signing Failure: {}", inner)
            }
            CeremonyFailureReason::DuplicateCeremonyId => {
                write!(f, "Duplicate Ceremony Id")
            }
            CeremonyFailureReason::ExpiredBeforeBeingAuthorized => {
                write!(f, "Ceremony expired before being authorized")
            }
        }
    }
}
