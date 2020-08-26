//! Witness has the following responsibilities:
//! - It is subscribed to the side chain for *quote transactions*
//! - It monitors foreign blockchains for *incoming transactions*

// Events: Lokid transaction, Ether transaction, Swap transaction from Side Chain

mod ethereum;
mod fake_witness;
mod loki_witness;

pub use ethereum::EthereumWitness;
pub use fake_witness::FakeWitness;
pub use loki_witness::LokiWitness;
