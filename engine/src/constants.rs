use std::time::Duration;

/// Number of blocks we wait until we deem it safe (from reorgs)
pub const ETH_BLOCK_SAFETY_MARGIN: u64 = 4;

// ======= Keygen and signing =======
//
/// Defines how long a signing ceremony remains pending. i.e. how long it waits for the key that is supposed to sign this message
/// to be generated. (Since we can receive requests to sign for the next key, if other nodes are ahead of us)
pub const PENDING_SIGN_DURATION: Duration = Duration::from_secs(500); // TODO Look at this value

/// Maximum duration a ceremony stage can last
pub const MAX_STAGE_DURATION: Duration = Duration::from_secs(300); // TODO Look at this value

// ======= State chain client =======

// ======= Eth Rpc Client =======
/// Duration before the attempt to connect to the ethereum node times out
pub const ETH_NODE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);
