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
        UNAUTHORIZED_KEYGEN_ABORTED, UNAUTHORIZED_SIGNING_ABORTED,
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
    pub validator_mapping: Arc<PartyIdxMapping>,
    pub params: ThresholdParameters,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyFailureReason<T> {
    #[error("Not participating in unauthorised ceremony")]
    NotParticipatingInUnauthorisedCeremony,
    #[error("Invalid Participants")]
    InvalidParticipants,
    #[error("Broadcast Failure ({0}) during {1} stage")]
    BroadcastFailure(BroadcastFailureReason, CeremonyStageName),
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

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CeremonyStageName {
    // Keygen
    #[error("Hash Commitments")]
    HashCommitments1,
    #[error("Verify Hash Commitments")]
    VerifyHashCommitmentsBroadcast2,
    #[error("Coefficient Commitments")]
    CoefficientCommitments3,
    #[error("Verify Coefficient Commitments")]
    VerifyCommitmentsBroadcast4,
    #[error("Secret Shares")]
    SecretSharesStage5,
    #[error("Complaints")]
    ComplaintsStage6,
    #[error("Verify Complaints")]
    VerifyComplaintsBroadcastStage7,
    #[error("Blame Responses")]
    BlameResponsesStage8,
    #[error("Verify Blame Responses")]
    VerifyBlameResponsesBroadcastStage9,

    // Signing
    #[error("Commitments")]
    AwaitCommitments1,
    #[error("Verify Commitments")]
    VerifyCommitmentsBroadcast2,
    #[error("Local Signatures")]
    LocalSigStage3,
    #[error("Verify Local Signatures")]
    VerifyLocalSigsBroadcastStage4,
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
            CeremonyFailureReason::NotParticipatingInUnauthorisedCeremony => {
                slog::warn!(logger,#UNAUTHORIZED_SIGNING_ABORTED, "{}: {}",SIGNING_CEREMONY_FAILED_PREFIX, self);
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
            CeremonyFailureReason::BroadcastFailure(_, _) => {
                slog::warn!(logger, #KEYGEN_CEREMONY_FAILED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self; REPORTED_PARTIES_KEY => reported_parties);
            }
            CeremonyFailureReason::NotParticipatingInUnauthorisedCeremony => {
                slog::warn!(logger,#UNAUTHORIZED_KEYGEN_ABORTED, "{}: {}",KEYGEN_CEREMONY_FAILED_PREFIX, self);
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
