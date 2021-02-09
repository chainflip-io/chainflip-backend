//! Witness has the following responsibilities:
//! - It is subscribed to the side chain for *quotes*
//! - It monitors foreign blockchains for *incoming transactions*

// Events: Oxend transaction, Ether transaction, Swap transaction from Side Chain

mod btc_spv;
mod ethereum;
mod oxen_witness;
mod witness_confirmer;

pub use btc_spv::BtcSPVWitness;
pub use ethereum::EthereumWitness;
pub use oxen_witness::OxenWitness;
pub use witness_confirmer::WitnessConfirmer;

/// A fake witness
pub mod fake_witness;
