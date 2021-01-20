

mod memory_local_store;
mod persistent_local_store;

use chainflip_common::types::chain::*;

use serde::{Deserialize, Serialize};

// pub use memory_local_store::MemoryLocalStore;
pub use persistent_local_store::PersistentLocalStore;

/// Side chain transaction type
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum LocalEvent {
    /// Witness
    Witness(Witness),
}

impl From<Witness> for LocalEvent {
    fn from(tx: Witness) -> Self {
        LocalEvent::Witness(tx)
    }
}

/// Interface that must be provided by any "side chain" implementation
pub trait ILocalStore {
    /// Add events to the local store
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String>;

}