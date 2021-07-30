use std::sync::Arc;

use cf_p2p::ValidatorId;
use serde::{Deserialize, Serialize};

use crate::signing::crypto::{Keys, Parameters, SharedKeys, VerifiableSS, GE};

use super::utils::ValidatorMaps;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeygenResult {
    pub keys: Keys,
    pub shared_keys: SharedKeys,
    pub vss: Vec<VerifiableSS<GE>>,
}

impl KeygenResult {
    pub fn get_public_key(&self) -> GE {
        self.shared_keys.y
    }
}

// TODO: combine the two Arcs?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeygenResultInfo {
    pub key: Arc<KeygenResult>,
    pub validator_map: Arc<ValidatorMaps>,
    pub params: Parameters,
}

impl KeygenResultInfo {
    pub fn get_idx(&self, id: &ValidatorId) -> Option<usize> {
        self.validator_map.get_idx(id)
    }

    pub fn get_id(&self, idx: usize) -> ValidatorId {
        // providing an invalid idx is considered a programmer error here
        self.validator_map
            .get_id(idx)
            .expect("invalid index")
            .clone()
    }
}
