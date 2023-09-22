pub use super::common::*;

// These represent approximately 10 minutes in localnet block times
// Bitcoin blocks are 5 seconds on localnets.
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 10 * 60 / 5;
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 10 * 60 / 14;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 10 * 60 / 6;
