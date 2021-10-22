use std::collections::HashMap;

use crate::multisig::{client::KeygenResultInfo, KeyId};

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
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo) {
        let val = bincode::serialize(key).expect("Should be serializable key");

        self.kv_db.insert(key_id.to_owned(), val);
    }

    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo> {
        self.kv_db
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    bincode::deserialize::<KeygenResultInfo>(v)
                        .expect("Invalid data for KeygenResultInfo"),
                )
            })
            .collect()
    }
}
