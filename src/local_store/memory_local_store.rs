use chainflip_common::types::{chain::Witness, UUIDv4};

use super::{ILocalStore, LocalEvent};

/// Fake implemenation of ILocalStore that stores events in memory
pub struct MemoryLocalStore {
    // For now store tx in memory:
    events: Vec<LocalEvent>,
}

impl MemoryLocalStore {
    /// Create an empty (fake) chain
    pub fn new() -> Self {
        MemoryLocalStore { events: vec![] }
    }
}

impl ILocalStore for MemoryLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        todo!()
    }

    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        todo!()
    }

    fn total_events(&mut self) -> u64 {
        todo!()
    }
}
