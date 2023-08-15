use std::time::Duration;

pub use state_chain_runtime::constants::common::eth::BLOCK_SAFETY_MARGIN as ETH_BLOCK_SAFETY_MARGIN;

pub use state_chain_runtime::constants::common::btc::BLOCK_SAFETY_MARGIN as BTC_INGRESS_BLOCK_SAFETY_MARGIN;

// ======= State chain client =======

/// Number of times to retry after incrementing the nonce on a nonce error
pub const MAX_EXTRINSIC_RETRY_ATTEMPTS: usize = 10;

// ======= Rpc Client Settings =======

pub const BLOCK_PULL_TIMEOUT_MULTIPLIER: u64 = 3;

// ======= Eth Rpc Client =======

/// Duration before we timeout an HTTP request to the Ethereum node
pub const ETH_HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Average time it takes to mine a block on Ethereum.
pub const ETH_AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(14);

/// Duration before we give up waiting on a response for a web3 request
pub const ETH_LOG_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the
/// chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

/// Number of blocks one of the protocols needs to fall behind before we sound the alarms
pub const ETH_FALLING_BEHIND_MARGIN_BLOCKS: u64 = 10;

/// Duration between intervals before we emit a log that one the ETH streams is behind
pub const ETH_STILL_BEHIND_LOG_INTERVAL: Duration = Duration::from_secs(180);

/// Number of blocks before logging that a stream is behind again
pub const ETH_LOG_BEHIND_REPORT_BLOCK_INTERVAL: u64 = 10;

// ======= Dot Rpc Client =======

pub const DOT_AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(6);

// ======= Settings environment variables =======

/// A HTTP node endpoint for Ethereum
pub const ETH_HTTP_NODE_ENDPOINT: &str = "ETH__HTTP_NODE_ENDPOINT";

/// A WebSocket node endpoint for Ethereum
pub const ETH_WS_NODE_ENDPOINT: &str = "ETH__WS_NODE_ENDPOINT";

pub const BTC_HTTP_NODE_ENDPOINT: &str = "BTC__HTTP_NODE_ENDPOINT";
pub const BTC_RPC_USER: &str = "BTC__RPC_USER";
pub const BTC_RPC_PASSWORD: &str = "BTC__RPC_PASSWORD";

pub const DOT_WS_NODE_ENDPOINT: &str = "DOT__WS_NODE_ENDPOINT";
pub const DOT_HTTP_NODE_ENDPOINT: &str = "DOT__HTTP_NODE_ENDPOINT";

/// IP Address and port on which we listen for incoming p2p connections
pub const NODE_P2P_IP_ADDRESS: &str = "NODE_P2P__IP_ADDRESS";
pub const NODE_P2P_PORT: &str = "NODE_P2P__PORT";

/// Base path for all files
pub const CONFIG_ROOT: &str = "CF_CONFIG_ROOT";
pub const DEFAULT_CONFIG_ROOT: &str = "/etc/chainflip";

/// Lifetime in blocks of submitted signed extrinsics
pub const SIGNED_EXTRINSIC_LIFETIME: state_chain_runtime::BlockNumber = 128;
