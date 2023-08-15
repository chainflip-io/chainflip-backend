use std::time::Duration;

// ======= Eth Rpc Client =======

/// Average time it takes to mine a block on Ethereum.
pub const ETH_AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(14);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the
/// chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

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
