use std::collections::HashMap;

use chainflip_common::types::{chain::*, UUIDv4};
use itertools::all;

use super::{ILocalStore, LocalEvent};

/// Fake implemenation of ILocalStore that stores events in memory
pub struct MemoryLocalStore {
    // Use transaction type enum instead of string
    events: HashMap<&'static str, Vec<LocalEvent>>,
}

impl MemoryLocalStore {
    /// Create an empty (fake) store
    pub fn new() -> Self {
        MemoryLocalStore {
            events: HashMap::new(),
        }
    }

    /// Convenience method for getting all witnesses
    pub fn get_witness_evts(&self) -> Vec<Witness> {
        let mut witnesses = Vec::new();
        let all_witnesses_from_db: Option<&Vec<LocalEvent>> = self.events.get("Witness");
        if all_witnesses_from_db.is_none() {
            return vec![];
        }
        for event in all_witnesses_from_db.unwrap() {
            match event {
                LocalEvent::Witness(w) => {
                    witnesses.push(w.clone());
                }
                _ => {
                    // skip
                }
            }
        }
        witnesses
    }
}

impl ILocalStore for MemoryLocalStore {
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        for event in events {
            match event {
                LocalEvent::Witness(_) => {
                    // store a witness
                    let empty = &mut Vec::new();
                    let witnesses = self.events.get("Witness").unwrap_or(empty);
                    let mut new_ws = witnesses.clone();
                    new_ws.push(event);
                    self.events.insert("Witness", new_ws);
                }
                LocalEvent::DepositQuote(_) => {}
                LocalEvent::Deposit(_) => {}
                LocalEvent::OutputSent(_) => {}
                LocalEvent::Output(_) => {}
                LocalEvent::PoolChange(_) => {}
                LocalEvent::SwapQuote(_) => {}
                LocalEvent::WithdrawRequest(_) => {}
                LocalEvent::Withdraw(_) => {}
            }
        }
        Ok(())
    }

    fn get_events(&mut self, last_seen: u64) -> Option<Vec<LocalEvent>> {
        let mut events = Vec::new();
        let all_witnesses: Option<&Vec<LocalEvent>> = self.events.get("Witness");
        if all_witnesses.is_none() {
            return None;
        }
        for event in all_witnesses.unwrap() {
            match event {
                LocalEvent::Witness(w) => {
                    if w.event_number.unwrap_or(0) > last_seen {
                        events.push(event.clone());
                    }
                }
                _ => {
                    todo!("More events");
                }
            }
        }

        Some(events)
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
