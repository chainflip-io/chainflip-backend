/// Reads events from state chain
pub mod sc_observer;

/// Submits events to state chain
pub mod sc_broadcaster;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

/// Contains helper methods for state chain code
mod helpers;

// TODO: Ensure all references to the state chain are general
// As this was previously called sc-observer, but now will contain
// both the observer and the broadcaster

// ==== Pallet support for the state chain =====

/// System pallet support for substrate-subxt but without the subxt bs
pub mod system;

/// Session pallet support for substrate-subxt, not dependent on Balances
pub mod session;
/// Staking pallet support for substrate-subxt
pub mod stake_manager;
/// Validator pallet support for substrate-subxt
pub mod validator;
