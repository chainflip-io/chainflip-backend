pub mod sc_observer;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

// ==== Pallet support for the sc-observer =====

/// Auction pallet support for substrate-subxt
pub mod auction;

/// Staking pallet support for substrate-subxt
pub mod staking;

/// Validator pallet support for substrate-subxt
pub mod validator;
