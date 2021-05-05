use self::{runtime::StateChainRuntime, staking::StakingEvent, validator::ValidatorEvent};

pub mod sc_observer;

/// The state chain runtime client type definitions
pub mod runtime;

mod sc_event;

// ==== Pallet support for the sc-observer =====

/// Staking pallet support for substrate-subxt
pub mod staking;

/// Validator pallet support for substrate-subxt
pub mod validator;
