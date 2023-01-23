pub mod client;
/// Reads events from state chain
mod sc_observer;

pub use sc_observer::{start, EthAddressToMonitorSender};
