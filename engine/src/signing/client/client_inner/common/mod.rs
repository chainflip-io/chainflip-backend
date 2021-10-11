pub mod broadcast;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

use tokio::sync::mpsc::UnboundedSender;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    p2p::{AccountId, P2PMessageCommand},
    signing::crypto::{KeyShare, Parameters, Point},
};

use super::{utils::ValidatorMaps, InnerEvent};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
        use crate::signing::crypto::ECPoint;
        self.key_share.y.get_element().serialize().into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeygenResultInfo {
    pub key: Arc<KeygenResult>,
    pub validator_map: Arc<ValidatorMaps>,
    pub params: Parameters,
}

impl KeygenResultInfo {
    pub fn get_idx(&self, id: &AccountId) -> Option<usize> {
        self.validator_map.get_idx(id)
    }

    pub fn get_id(&self, idx: usize) -> AccountId {
        self.validator_map
            .get_id(idx)
            .expect("ProgrammerError, invalid index")
            .clone()
    }
}

/// Able to send `Data` to the party identified
/// by signer idx
pub trait P2PSender: Clone {
    type Data;

    fn send(&self, idx: usize, data: Self::Data);
}

/// Sends raw data (bytes) through a channel
/// (additionally mapping signer idx to account id)
#[derive(Clone)]
pub struct RawP2PSender {
    validator_map: Arc<ValidatorMaps>,
    inner_event_sender: UnboundedSender<InnerEvent>,
}

impl RawP2PSender {
    pub fn new(
        validator_map: Arc<ValidatorMaps>,
        inner_event_sender: UnboundedSender<InnerEvent>,
    ) -> Self {
        RawP2PSender {
            validator_map,
            inner_event_sender,
        }
    }

    pub fn send(&self, idx: usize, data: Vec<u8>) {
        let id = self
            .validator_map
            .get_id(idx)
            .expect("`idx` should carefully selected by caller")
            .clone();

        let msg = P2PMessageCommand::new(id, data);

        self.inner_event_sender.send(msg.into()).unwrap()
    }
}
