use chainflip_common::types::{chain::*, UUIDv4};

use super::{ILocalStore, LocalEvent};

/// Fake implemenation of ILocalStore that stores events in memory
pub struct MemoryLocalStore {
    // Use transaction type enum instead of string
    events: Vec<LocalEvent>,
}

impl MemoryLocalStore {
    /// Create an empty (fake) store
    pub fn new() -> Self {
        MemoryLocalStore { events: vec![] }
    }

    /// Helper for getting just the witness transactions
    pub fn get_witness_evts(&self) -> Vec<Witness> {
        let witness_events: Vec<_> = self
            .events
            .iter()
            .filter(|e| {
                if let LocalEvent::Witness(w) = e {
                    true
                } else {
                    false
                }
            })
            .collect();

        let mut witnesses: Vec<Witness> = vec![];

        for witness in witness_events {
            if let LocalEvent::Witness(w) = witness {
                witnesses.push(w.clone());
            }
        }

        witnesses
    }
}

impl ILocalStore for MemoryLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        for event in &events {
            // don't add duplicates
            if events.iter().any(|e| e == event) {
                self.events.push(event.clone());
            }
        }
        Ok(())
    }

    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        if self.events.is_empty() {
            return None;
        }
        // let slice_num = last_seen + 1;
        Some(self.events[last_seen as usize..].to_vec())
    }

    fn total_events(&mut self) -> u64 {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::{self, data::TestData};
    use chainflip_common::types::coin::Coin;

    #[test]
    fn add_events() {
        let mut store = MemoryLocalStore::new();
        let witness = Witness {
            id: UUIDv4::new(),
            quote: UUIDv4::new(),
            transaction_id: "".into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 0,
            coin: Coin::BTC,
            event_number: Some(1),
        };
        let witness = LocalEvent::Witness(witness);
        let events = vec![witness];
        store.add_events(events).unwrap();

        let stored_events = store.get_events(0).unwrap();
        assert_eq!(stored_events.len(), 1);
    }
}
