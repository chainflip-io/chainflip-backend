#[cfg(test)]
mod mock;
mod persistent;

use std::collections::{HashMap, HashSet};

#[cfg(test)]
pub use mock::MultisigDBMock;
use pallet_cf_vaults::CeremonyId;
pub use persistent::PersistentMultisigDB;

use super::{client::KeygenResultInfo, KeyId};

pub trait MultisigDB {
    /// Save a new (or update an existing) entry from the underlying storage
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo);

    /// Load all existing keys from the underlying storage
    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo>;

    /// Save a new unused ceremony id to the underlying storage
    fn save_used_ceremony_id(&mut self, ceremony_id: CeremonyId, db_colum: u32);

    /// Delete the unused ceremony id from the underlying storage
    fn remove_used_ceremony_id(&mut self, ceremony_id: &CeremonyId, db_colum: u32);

    /// Load all the unused ceremony ids from the underlying storage
    fn load_used_ceremony_ids(&self, db_colum: u32) -> HashSet<CeremonyId>;
}
