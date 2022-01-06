#[cfg(test)]
mod mock;
mod persistent;

use std::collections::{HashMap, HashSet};

#[cfg(test)]
pub use mock::KeyDBMock;
use pallet_cf_vaults::CeremonyId;
pub use persistent::PersistentKeyDB;

use super::{client::KeygenResultInfo, KeyId};

pub trait KeyDB {
    /// Save a new (or update an existing) entry from the underlying storage
    fn update_key(&mut self, key_id: &KeyId, key: &KeygenResultInfo);

    /// Load all existing keys from the underlying storage
    fn load_keys(&self) -> HashMap<KeyId, KeygenResultInfo>;

    /// Save a new unused ceremony id to the underlying storage
    fn update_tracking_for_signing(&mut self, data: &HashSet<CeremonyId>);
    fn update_tracking_for_keygen(&mut self, data: &HashSet<CeremonyId>);

    /// Load all the unused ceremony ids from the underlying storage
    fn load_tracking_for_signing(&self) -> HashSet<CeremonyId>;
    fn load_tracking_for_keygen(&self) -> HashSet<CeremonyId>;
}
