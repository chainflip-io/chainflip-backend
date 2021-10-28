#[cfg(test)]
mod mock;
mod persistent;

use std::collections::HashMap;

#[cfg(test)]
pub use mock::KeyDBMock;
pub use persistent::PersistentKeyDB;

use super::{client::KeygenResultInfo, KeyId};

pub trait KeyDB {
    /// Save a new (or update an existing) entry from the underlying storage
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo);

    /// Load all existing keys from the underlying storage
    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo>;
}
