/// Should only be one of these in the final PR, this is to try them out
pub mod sc_observer;

// ==== Pallet support for the sc-observer =====

/// Staking pallet support for substrate-subxt
pub mod staking;

/// Validator pallet support for substrate-subxt
pub mod validator;

/// The state chain runtime client type definitions
pub mod runtime;
