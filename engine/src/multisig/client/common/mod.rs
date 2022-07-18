pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{
    CeremonyCommon, CeremonyStage, PreProcessStageDataCheck, ProcessMessageResult, StageResult,
};

pub use broadcast_verification::BroadcastVerificationMessage;
use state_chain_runtime::AccountId;

use std::{collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    common::format_iterator,
    logging::{
        KEYGEN_CEREMONY_FAILED, KEYGEN_REJECTED_INCOMPATIBLE, KEYGEN_REQUEST_IGNORED,
        REPORTED_PARTIES_KEY, REQUEST_TO_SIGN_IGNORED, SIGNING_CEREMONY_FAILED,
        UNAUTHORIZED_KEYGEN_EXPIRED, UNAUTHORIZED_SIGNING_EXPIRED,
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
    #[error("Secret Shares")]
    SecretShares,
}

const SIGNING_CEREMONY_FAILED_PREFIX: &str = "Signing ceremony failed";
const KEYGEN_CEREMONY_FAILED_PREFIX: &str = "Keygen ceremony failed";
const REQUEST_TO_SIGN_IGNORED_PREFIX: &str = "Signing request ignored";
const KEYGEN_REQUEST_IGNORED_PREFIX: &str = "Keygen request ignored";

impl CeremonyFailureReason<SigningFailureReason> {
    pub fn log(&self, reported_parties: &BTreeSet<AccountId>, logger: &slog::Logger) {
        let reported_parties = format_iterator(reported_parties).to_string();
        match self {
            CeremonyFailureReason::BroadcastFailure(_, _) => {
                slog::warn!(logger, #SIGNING_CEREMONY_FAILED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
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
            CeremonyFailureReason::Other(SigningFailureReason::InvalidSigShare) => {
                slog::warn!(logger, #SIGNING_CEREMONY_FAILED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
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
    pub fn log(&self, reported_parties: &BTreeSet<AccountId>, logger: &slog::Logger) {
        let reported_parties = format_iterator(reported_parties).to_string();
        match self {
            CeremonyFailureReason::DuplicateCeremonyId => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "{}: {}",KEYGEN_REQUEST_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::BroadcastFailure(_, _) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
            }
            CeremonyFailureReason::ExpiredBeforeBeingAuthorized => {
                slog::warn!(logger,#UNAUTHORIZED_KEYGEN_EXPIRED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
            CeremonyFailureReason::InvalidParticipants => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "{}: {}",KEYGEN_REQUEST_IGNORED_PREFIX, self);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidBlameResponse) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidCommitment) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::InvalidComplaint) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
            }
            CeremonyFailureReason::Other(KeygenFailureReason::KeyNotCompatible) => {
                slog::debug!(logger, #KEYGEN_REJECTED_INCOMPATIBLE, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
            }
        }
    }
}
