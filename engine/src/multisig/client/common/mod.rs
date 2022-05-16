pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

pub use broadcast_verification::{BroadcastFailureReason, BroadcastVerificationMessage};

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::multisig::crypto::{ECPoint, KeyShare};

use super::{utils::PartyIdxMapping, ThresholdParameters};

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
