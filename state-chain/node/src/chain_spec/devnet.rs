use cf_primitives::AuthorityCount;
use state_chain_runtime::SetSizeParameters;

pub use super::common::*;

// These represent approximately 10 minutes in localnet block times
// Bitcoin blocks are 5 seconds on localnets.
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 10 * 60 / 5;
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 10 * 60 / 14;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 10 * 60 / 6;
pub const SOLANA_EXPIRY_BLOCKS: u32 = 10 * 60 * 10 / 4;

pub const MIN_AUTHORITIES: AuthorityCount = 1;
pub const AUCTION_PARAMETERS: SetSizeParameters = SetSizeParameters {
	min_size: MIN_AUTHORITIES,
	max_size: MAX_AUTHORITIES,
	max_expansion: MAX_AUTHORITIES,
};

pub const BITCOIN_SAFETY_MARGIN: u64 = 2;
pub const ETHEREUM_SAFETY_MARGIN: u64 = 2;
