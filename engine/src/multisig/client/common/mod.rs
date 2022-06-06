pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use broadcast_verification::BroadcastVerificationMessage;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    logging::{
        KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, KEYGEN_REQUEST_IGNORED,
        REQUEST_TO_SIGN_IGNORED, SIGNING_CEREMONY_FAILED, UNAUTHORIZED_KEYGEN_EXPIRED,
        UNAUTHORIZED_SIGNING_EXPIRED,
    },
    multisig::crypto::{ECPoint, KeyShare},
};

use super::{utils::PartyIdxMapping, ThresholdParameters};

use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResult<P: ECPoint> {
    #[serde(bound = "")]
    pub key_share: KeyShare<P>,
    #[serde(bound = "")]
    pub party_public_keys: Vec<P>,
}

impl<P: ECPoint> KeygenResult<P> {
    pub fn get_public_key(&self) -> P {
        self.key_share.y
    }

    /// Gets the serialized compressed public key (33 bytes - 32 bytes + a y parity byte)
    pub fn get_public_key_bytes(&self) -> Vec<u8> {
        self.key_share.y.as_bytes().as_ref().into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResultInfo<P: ECPoint> {
    #[serde(bound = "")]
    pub key: Arc<KeygenResult<P>>,
    pub validator_map: Arc<PartyIdxMapping>,
    pub params: ThresholdParameters,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyFailureReason<T> {
    #[error("Duplicate Ceremony Id")]
    DuplicateCeremonyId,
    #[error("Expired before being authorized")]
    ExpiredBeforeBeingAuthorized,
    #[error("Invalid Participants")]
    InvalidParticipants,
    #[error("Broadcast Failure ({0}) during {1} stage")]
    BroadcastFailure(BroadcastFailureReason, BroadcastStageName),
    #[error("{0}")]
    Other(T),
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigningFailureReason {
    #[error("Invalid Sig Share")]
    InvalidSigShare,
    #[error("Not Enough Signers")]
    NotEnoughSigners,
    #[error("Unknown Key")]
    UnknownKey,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenFailureReason {
    #[error("Invalid Commitment")]
    InvalidCommitment,
    #[error("Invalid secret share in a blame response")]
    InvalidBlameResponse,
    #[error("The key is not compatible")]
    KeyNotCompatible,
    #[error("Invalid Complaint")]
    InvalidComplaint,
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
    #[error("Coefficient Commitments")]
    CoefficientCommitments,
    #[error("Local Signatures")]
    LocalSignatures,
    #[error("Hash Commitments")]
    HashCommitments,
    #[error("Complaints")]
    Complaints,
    #[error("Blame Responses")]
    BlameResponses,
}

const SIGNING_CEREMONY_FAILED_PREFIX: &str = "Signing ceremony failed";
const KEYGEN_CEREMONY_FAILED_PREFIX: &str = "Keygen ceremony failed";
const REQUEST_TO_SIGN_IGNORED_PREFIX: &str = "Signing request ignored";
const KEYGEN_REQUEST_IGNORED_PREFIX: &str = "Keygen request ignored";

impl CeremonyFailureReason<SigningFailureReason> {
    pub fn log(&self, logger: &slog::Logger) {
        match self {
            CeremonyFailureReason::BroadcastFailure(_, _) => {
                slog::warn!(logger, #SIGNING_CEREMONY_FAILED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::DuplicateCeremonyId => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "{}: {}",REQUEST_TO_SIGN_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::ExpiredBeforeBeingAuthorized => {
                slog::warn!(logger,#UNAUTHORIZED_SIGNING_EXPIRED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::InvalidParticipants => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "{}: {}",REQUEST_TO_SIGN_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::CeremonyIdAlreadyUsed => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "{}: {}",REQUEST_TO_SIGN_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::Other(SigningFailureReason::InvalidSigShare) => {
                slog::warn!(logger, #SIGNING_CEREMONY_FAILED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::Other(SigningFailureReason::NotEnoughSigners) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "{}: {}",REQUEST_TO_SIGN_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::Other(SigningFailureReason::UnknownKey) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "{}: {}",REQUEST_TO_SIGN_IGNORED_PREFIX, self);
            }
        }
    }
}

impl CeremonyFailureReason<KeygenFailureReason> {
    pub fn log(&self, logger: &slog::Logger) {
        match self {
            CeremonyFailureReason::DuplicateCeremonyId => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "{}: {}",KEYGEN_REQUEST_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::BroadcastFailure(_, _) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::ExpiredBeforeBeingAuthorized => {
                slog::warn!(logger,#UNAUTHORIZED_KEYGEN_EXPIRED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::InvalidParticipants => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "{}: {}",KEYGEN_REQUEST_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::CeremonyIdAlreadyUsed => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "{}: {}",KEYGEN_REQUEST_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidCommitment) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidComplaint) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::KeyNotCompatible) => {
                slog::debug!(logger, #KEYGEN_REJECTED_INCOMPATIBLE, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
        }
    }
}

// Test that a few of the failure reasons are logging the correct tags
#[test]
fn test_failure_reason_logs_with_tag() {
    let (logger, mut tag_cache) = crate::logging::test_utils::new_test_logger_with_tag_cache();

    CeremonyFailureReason::DuplicateCeremonyId::<SigningFailureReason>.log(&logger);
    assert!(tag_cache.contains_tag(REQUEST_TO_SIGN_IGNORED));

    tag_cache.clear();

    CeremonyFailureReason::CeremonyIdAlreadyUsed::<KeygenFailureReason>.log(&logger);
    assert!(tag_cache.contains_tag(KEYGEN_REQUEST_IGNORED));

    tag_cache.clear();

    CeremonyFailureReason::Other(SigningFailureReason::InvalidSigShare).log(&logger);
    assert!(tag_cache.contains_tag(SIGNING_CEREMONY_FAILED));

    tag_cache.clear();

    CeremonyFailureReason::BroadcastFailure::<KeygenFailureReason>(
        BroadcastFailureReason::InsufficientMessages,
        BroadcastStageName::BlameResponses,
    )
    .log(&logger);
    assert!(tag_cache.contains_tag(KEYGEN_CEREMONY_FAILED));
}
