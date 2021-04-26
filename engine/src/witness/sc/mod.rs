use codec::Decode;

/// State chain witnesser
pub mod sc_witness;

// Should only be one of these in the final PR, this is to try them out
pub mod subxt_witness;

// types for the client
pub mod transactions;

/// The state chain runtime client type definitions
pub mod runtime;

pub trait Event<T>: Decode {
    /// Module name.
    const MODULE: &'static str;
    /// Event name.
    const EVENT: &'static str;
}
