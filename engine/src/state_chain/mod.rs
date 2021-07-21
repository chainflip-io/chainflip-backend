/// Reads events from state chain
pub mod sc_observer;

/// Submits events to state chain
pub mod sc_broadcaster;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

/// Contains helper methods for state chain code
pub mod helpers;

pub use helpers::get_signer_from_privkey_file;

// ==== Pallet support for the state chain =====

/// Auction pallet support for substrate-subxt
pub mod auction;

/// Staking pallet support for substrate-subxt
pub mod staking;

/// Validator pallet support for substrate-subxt
pub mod validator;

/// Session pallet support for substrate-subxt
/// NB: Can't use subxt's default because it uses Balances
pub mod session;
