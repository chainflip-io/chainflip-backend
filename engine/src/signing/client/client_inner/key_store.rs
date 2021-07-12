use std::collections::HashMap;

use crate::signing::KeyId;

use super::common::KeygenResultInfo;

// Successfully generated multisig keys live here
#[derive(Clone)]
pub struct KeyStore {
    keys: HashMap<KeyId, KeygenResultInfo>,
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore {
            keys: HashMap::new(),
        }
    }

    pub fn get_key(&self, key_id: KeyId) -> Option<&KeygenResultInfo> {
        self.keys.get(&key_id)
    }

    // Save `key` under key `key_id` overwriting if exists
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo) {
        self.keys.insert(key_id, key);
    }
}
