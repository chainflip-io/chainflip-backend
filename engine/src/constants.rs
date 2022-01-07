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

/// Number of times to retry after incrementing the nonce on a nonce error
pub const MAX_RETRY_ATTEMPTS: usize = 10;
