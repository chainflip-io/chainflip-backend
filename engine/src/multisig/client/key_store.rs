use std::collections::HashMap;

use crate::multisig::{crypto::ECPoint, KeyDB, KeyId};

use super::common::KeygenResultInfo;

// TODO: do we want all keys (irrespective of the blockchain/scheme) to
// be stored in the same database?
// Successfully generated multisig keys live here
pub struct KeyStore<S, P>
where
    S: KeyDB<P>,
    P: ECPoint,
{
    keys: HashMap<KeyId, KeygenResultInfo<P>>,
    db: S,
}

impl<S, P> KeyStore<S, P>
where
    S: KeyDB<P>,
    P: ECPoint,
{
    pub fn new(db: S) -> Self {
        let keys = db.load_keys();

        KeyStore { keys, db }
    }

    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo<P>> {
        self.keys.get(key_id)
    }

    // Save `key` under key `key_id` overwriting if exists
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<P>) {
        self.db.update_key(&key_id, &key);
        self.keys.insert(key_id, key);
    }
}
