use std::vec::Vec;

/// The types for the chain states
pub mod chain;

/// A simple timestamp
mod timestamp;
pub use timestamp::Timestamp;

/// Coin information
pub mod coin;

/// Fraction wrappers
pub mod fraction;

/// Address types
pub mod addresses;

/// Utf8 string
pub mod utf8;

/// Unique id
pub mod unique_id;

/// Network types
mod network;
pub use network::*;

// Common Types

pub type Bytes = Vec<u8>;
pub type AtomicAmount = u128;
