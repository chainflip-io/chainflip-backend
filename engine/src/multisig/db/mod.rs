pub mod persistent;

mod migrations;

use std::collections::HashMap;

pub use persistent::PersistentKeyDB;

use super::{client::KeygenResultInfo, crypto::ECPoint, KeyId};

pub trait KeyDB<P: ECPoint> {
    /// Save a new (or update an existing) entry from the underlying storage
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo<P>);

    /// Load all existing keys from the underlying storage
    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo<P>>;
}
