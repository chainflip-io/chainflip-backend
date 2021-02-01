mod memory_local_store;
mod persistent_local_store;

use chainflip_common::types::chain::*;

use serde::{Deserialize, Serialize};

pub use memory_local_store::MemoryLocalStore;
pub use persistent_local_store::PersistentLocalStore;

/// Side chain transaction type
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum LocalEvent {
    /// Witness
    Witness(Witness),
    /// Deposit quote
    DepositQuote(DepositQuote),
    /// Deposit
    Deposit(Deposit),
    /// Output sent
    OutputSent(OutputSent),
    /// Output
    Output(Output),
    /// Pool change
    PoolChange(PoolChange),
    /// Swap quote
    SwapQuote(SwapQuote),
    /// Withdraw request
    WithdrawRequest(WithdrawRequest),
    /// Withdraw
    Withdraw(Withdraw),
}

impl From<Witness> for LocalEvent {
    fn from(tx: Witness) -> Self {
        LocalEvent::Witness(tx)
    }
}

impl From<DepositQuote> for LocalEvent {
    fn from(tx: DepositQuote) -> Self {
        LocalEvent::DepositQuote(tx)
    }
}

impl From<Deposit> for LocalEvent {
    fn from(tx: Deposit) -> Self {
        LocalEvent::Deposit(tx)
    }
}

impl From<OutputSent> for LocalEvent {
    fn from(tx: OutputSent) -> Self {
        LocalEvent::OutputSent(tx)
    }
}

impl From<Output> for LocalEvent {
    fn from(tx: Output) -> Self {
        LocalEvent::Output(tx)
    }
}

impl From<PoolChange> for LocalEvent {
    fn from(tx: PoolChange) -> Self {
        LocalEvent::PoolChange(tx)
    }
}

impl From<SwapQuote> for LocalEvent {
    fn from(tx: SwapQuote) -> Self {
        LocalEvent::SwapQuote(tx)
    }
}

impl From<WithdrawRequest> for LocalEvent {
    fn from(tx: WithdrawRequest) -> Self {
        LocalEvent::WithdrawRequest(tx)
    }
}

impl From<Withdraw> for LocalEvent {
    fn from(tx: Withdraw) -> Self {
        LocalEvent::Withdraw(tx)
    }
}

/// Gets the event number for a local event
pub trait GetEventNumber {
    /// Gets the event number for a local event
    fn event_number(&self) -> Option<u64>;
}

impl GetEventNumber for LocalEvent {
    fn event_number(&self) -> Option<u64> {
        match self {
            LocalEvent::Witness(e) => e.event_number,
            LocalEvent::DepositQuote(e) => e.event_number,
            LocalEvent::Deposit(e) => e.event_number,
            LocalEvent::OutputSent(e) => e.event_number,
            LocalEvent::Output(e) => e.event_number,
            LocalEvent::PoolChange(e) => e.event_number,
            LocalEvent::SwapQuote(e) => e.event_number,
            LocalEvent::WithdrawRequest(e) => e.event_number,
            LocalEvent::Withdraw(e) => e.event_number,
        }
    }
}

/// Interface that must be provided by any "side chain" implementation
pub trait ILocalStore {
    /// Add events to the local store
    fn add_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String>;

    /// Get events from the local store
    fn get_events(&self, last_event: u64) -> Vec<LocalEvent>;

    /// Helper method for getting just the witnesses
    fn get_witnesses(&self, last_event: u64) -> Vec<Witness> {
        let witnesses: Vec<_> = self
            .get_events(last_event)
            .iter()
            .filter_map(|event| {
                if let LocalEvent::Witness(w) = event {
                    Some(w.clone())
                } else {
                    None
                }
            })
            .collect();

        witnesses
    }

    /// Get total number of events in the db
    fn total_events(&self) -> u64;
}

/// Trait for items to be stored in the database
pub trait StorageItem {
    /// Generate a unique id for use as Key in the Sqlite DB
    fn unique_id(&self) -> String;
}

// Must be unique across *all LocalEvents*
impl StorageItem for LocalEvent {
    fn unique_id(&self) -> String {
        match self {
            LocalEvent::Withdraw(evt) => evt.id.to_string(),
            LocalEvent::Witness(evt) => {
                format!("{}-{}", evt.coin.to_string(), evt.transaction_id)
            }
            LocalEvent::DepositQuote(evt) => evt.id.to_string(),
            LocalEvent::Deposit(evt) => evt.id.to_string(),
            LocalEvent::OutputSent(evt) => evt.id.to_string(),
            LocalEvent::Output(evt) => evt.id.to_string(),
            LocalEvent::PoolChange(evt) => evt.id.to_string(),
            LocalEvent::SwapQuote(evt) => evt.id.to_string(),
            LocalEvent::WithdrawRequest(evt) => evt.id.to_string(),
        }
    }
}
