/// Reads events from state chain
pub mod sc_observer;

/// Submits events to state chain
pub mod sc_broadcaster;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

/// Contains helper methods for state chain code
mod helpers;

/// Contains method to get the nodes state chain id on initialisation
mod node_id;

// ==== Pallet support for the state chain =====

/// Auction pallet support for substrate-subxt
pub mod auction;

/// Staking pallet support for substrate-subxt
pub mod staking;
/// Validator pallet support for substrate-subxt
pub mod validator;
