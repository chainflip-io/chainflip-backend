use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::multisig::{KeyId, MultisigDB};

use super::common::KeygenResultInfo;

// Successfully generated multisig keys live here
#[derive(Clone)]
pub struct KeyStore<S>
where
    S: MultisigDB,
{
    keys: HashMap<KeyId, KeygenResultInfo>,
    db: Arc<Mutex<S>>,
}

impl<S> KeyStore<S>
where
    S: MultisigDB,
{
    pub fn new(db: Arc<Mutex<S>>) -> Self {
        let keys = db.lock().unwrap().load_keys();

        KeyStore { keys, db }
    }

    #[cfg(test)]
    pub fn get_db(&self) -> Arc<Mutex<S>> {
        self.db.clone()
    }

    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo> {
        self.keys.get(key_id)
    }

    // Save `key` under key `key_id` overwriting if exists
    // TODO: Can we borrow KeyId here too?
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo) {
        self.db.lock().unwrap().update_key(&key_id, &key);
        self.keys.insert(key_id, key);
    }
}
