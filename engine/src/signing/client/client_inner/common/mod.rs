pub mod broadcast;
mod ceremony_stage;

pub use ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};

use tokio::sync::mpsc::UnboundedSender;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    p2p::{AccountId, P2PMessageCommand},
    signing::crypto::{KeyShare, Parameters, GE as Point},
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

// TODO: combine the two Arcs? (Actually, I think it is easier we we just clone everything)
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
        // providing an invalid idx is considered a programmer error here
        self.validator_map
            .get_id(idx)
            .expect("invalid index")
            .clone()
    }
}

#[derive(Clone)]
pub struct P2PSender {
    validator_map: Arc<ValidatorMaps>,
    sender: UnboundedSender<InnerEvent>,
}

impl P2PSender {
    pub fn new(validator_map: Arc<ValidatorMaps>, sender: UnboundedSender<InnerEvent>) -> Self {
        P2PSender {
            validator_map,
            sender,
        }
    }

    pub fn send(&self, idx: usize, data: Vec<u8>) {
        let id = self.validator_map.get_id(idx).unwrap().clone();

        // combine id and serialized data
        let msg = P2PMessageCommand::new(id, data);

        // We could use `into()` here
        let event = InnerEvent::P2PMessageCommand(msg);

        if let Err(err) = self.sender.send(event) {
            eprintln!("Could not send p2p message: {}", err);
            // slog::error!(self.logger, "Could not send p2p message: {}", err);
        }
    }
}
