use std::time::Duration;

/// Number of blocks we wait until we deem it safe (from reorgs)
pub const ETH_BLOCK_SAFETY_MARGIN: u64 = 4;

// ======= State chain client =======

/// Number of times to retry after incrementing the nonce on a nonce error
pub const MAX_EXTRINSIC_RETRY_ATTEMPTS: usize = 10;

// ======= Eth Rpc Client =======
/// Duration before the attempt to connect to the ethereum node times out
pub const ETH_NODE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Duration before we give up waiting on a response for a web3 request
pub const ETH_LOG_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Duration before we timeout a select_ok request to both http and ws
pub const ETH_DUAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

/// Number of blocks one of the protocols needs to fall behind before we sound the alarms
pub const ETH_FALLING_BEHIND_MARGIN_BLOCKS: u64 = 10;

/// Duration between intervals before we emit a log that one the ETH streams is behind
pub const ETH_STILL_BEHIND_LOG_INTERVAL: Duration = Duration::from_secs(180);

/// Number of blocks before logging that a stream is behind again
pub const ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL: u64 = 10;
