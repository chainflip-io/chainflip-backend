pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use broadcast_verification::BroadcastVerificationMessage;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::multisig::crypto::{ECPoint, KeyShare};

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
