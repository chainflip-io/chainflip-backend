use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    p2p::AccountId,
    signing::crypto::{ECPoint, Keys, Parameters, SharedKeys, VerifiableSS, GE},
};

use super::utils::ValidatorMaps;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeygenResult {
    pub keys: Keys,
    pub shared_keys: SharedKeys,
    pub vss: Vec<VerifiableSS<GE>>,
}

impl std::fmt::Display for KeygenResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.get_public_key_bytes()))
    }
}

impl KeygenResult {
    pub fn get_public_key(&self) -> GE {
        self.shared_keys.y
    }

    /// Gets the serialized compressed public key (33 bytes - 32 bytes + a y parity byte)
    pub fn get_public_key_bytes(&self) -> Vec<u8> {
        self.shared_keys.y.get_element().serialize().into()
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
