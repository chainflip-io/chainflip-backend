/// The amount of LOKI to charge for processing a swap
pub const LOKI_SWAP_PROCESS_FEE: u128 = 500_000_000; // 0.5 LOKI

/// The swap quote exipiry time in milliseconds after which we discard it
pub const SWAP_QUOTE_HARD_EXPIRE: u128 = 30 * 24 * 60 * 60 * 1000; // 30 days

/// The swap quote expiry time in milliseconds after which we refund the user
pub const SWAP_QUOTE_EXPIRE: u128 = 12 * 60 * 60 * 1000; // 12 hours
