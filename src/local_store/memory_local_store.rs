use chainflip_common::types::chain::*;

use super::{ILocalStore, LocalEvent};

/// Fake implemenation of ILocalStore that stores events in memory
pub struct MemoryLocalStore {
    events: Vec<LocalEvent>,
}

impl MemoryLocalStore {
    /// Create an empty (fake) store
    pub fn new() -> Self {
        MemoryLocalStore { events: vec![] }
    }

    /// Helper for getting just the witnesses
    pub fn get_witness_evts(&self) -> Vec<Witness> {
        let witness_events: Vec<_> = self
            .events
            .iter()
            .filter(|e| {
                if let LocalEvent::Witness(_) = e {
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
        for new_event in &events {
            // don't add duplicates
            if !self.events.iter().any(|e| e == new_event) {
                self.events.push(new_event.clone());
            }
        }
        Ok(())
    }

    fn get_events(&self, last_seen: u64) -> Vec<LocalEvent> {
        self.events[last_seen as usize..].to_vec()
    }

    fn total_events(&self) -> u64 {
        self.events.len() as u64
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::data::TestData;
    use chainflip_common::types::coin::Coin;

    #[test]
    fn add_events() {
        let mut store = MemoryLocalStore::new();
        let witness = Witness {
            quote: 0,
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

        let stored_events = store.get_events(0);
        assert_eq!(stored_events.len(), 1);
    }

    #[test]
    fn add_events_no_dups() {
        let mut store = MemoryLocalStore::new();
        let witness = TestData::witness(0, 100, Coin::ETH);
        store
            .add_events(vec![witness.clone().into(), witness.into()])
            .unwrap();
        assert_eq!(store.get_events(0).len(), 1);
    }

    #[test]
    fn get_events_from_last_seen() {
        let mut store = MemoryLocalStore::new();
        let evt = LocalEvent::Witness(TestData::witness(0, 1000, Coin::ETH));
        let dq = LocalEvent::DepositQuote(TestData::deposit_quote(Coin::ETH));

        store.add_events(vec![evt, dq]).unwrap();

        let all_events = store.get_events(1);
        assert_eq!(all_events.len(), 1);
    }

    #[test]
    fn get_total_events() {
        let mut store = MemoryLocalStore::new();
        let evt = LocalEvent::Witness(TestData::witness(0, 1000, Coin::ETH));
        let dq = LocalEvent::DepositQuote(TestData::deposit_quote(Coin::ETH));

        store.add_events(vec![evt, dq]).unwrap();

        assert_eq!(store.total_events(), 2);
    }
}
