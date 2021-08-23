use std::collections::HashMap;

use crate::signing::{client::KeygenResultInfo, KeyId};

use super::KeyDB;

#[derive(Clone)]
pub struct KeyDBMock {
    // Represents a key-value database
    kv_db: HashMap<KeyId, Vec<u8>>,
}

impl KeyDBMock {
    pub fn new() -> Self {
        KeyDBMock {
            kv_db: HashMap::new(),
        }
    }
}

impl KeyDB for KeyDBMock {
    fn update_key(&mut self, key_id: KeyId, key: &KeygenResultInfo) {
        let val = bincode::serialize(key).unwrap();

        self.kv_db.insert(key_id, val);
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.kv_db
            .iter()
            .map(|(k, v)| (*k, bincode::deserialize(v).unwrap()))
            .collect()
    }
}
