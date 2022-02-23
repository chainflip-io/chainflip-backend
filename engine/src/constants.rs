use std::time::Duration;

/// Number of blocks we wait until we deem it safe (from reorgs)
pub const ETH_BLOCK_SAFETY_MARGIN: u64 = 4;

// ======= Keygen and signing =======
//
/// Defines how long a signing ceremony remains pending. i.e. how long it waits for the key that is supposed to sign this message
/// to be generated. (Since we can receive requests to sign for the next key, if other nodes are ahead of us)
pub const PENDING_SIGN_DURATION: Duration = Duration::from_secs(500); // TODO Look at this value

/// Maximum duration a ceremony stage can last
pub const MAX_STAGE_DURATION: Duration = Duration::from_secs(120); // TODO Look at this value

// ======= State chain client =======

/// Number of times to retry after incrementing the nonce on a nonce error
pub const MAX_RETRY_ATTEMPTS: usize = 10;

// ======= Eth Rpc Client =======
/// Duration before the attempt to connect to the ethereum node times out
pub const ETH_NODE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

/// Number of blocks one of the protocols needs to fall behind before we sound the alarms
pub const ETH_FALLING_BEHIND_MARGIN_BLOCKS: u64 = 10;

/// Number of blocks before logging that a stream is behind again
pub const ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL: u64 = 10;

/// Duration before we give up waiting on a response for a web3 request
pub const WEB3_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
