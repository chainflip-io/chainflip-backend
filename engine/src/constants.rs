use std::time::Duration;

// ======= Eth Rpc Client =======

/// Average time it takes to mine a block on Ethereum.
pub const ETH_AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(14);

/// Duration between each poll of the web3 client, to check if we are synced to the head of the
/// chain
pub const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

// ======= Dot Rpc Client =======

pub const DOT_AVERAGE_BLOCK_TIME: Duration = Duration::from_secs(6);

// ======= Rpc Clients =======

pub const RPC_RETRY_CONNECTION_INTERVAL: Duration = Duration::from_secs(10);

// ======= Settings environment variables =======

pub const ETH_HTTP_ENDPOINT: &str = "ETH__RPC__HTTP_ENDPOINT";
pub const ETH_WS_ENDPOINT: &str = "ETH__RPC__WS_ENDPOINT";

pub const ETH_BACKUP_HTTP_ENDPOINT: &str = "ETH__BACKUP_RPC__HTTP_ENDPOINT";
pub const ETH_BACKUP_WS_ENDPOINT: &str = "ETH__BACKUP_RPC__WS_ENDPOINT";

pub const BTC_HTTP_ENDPOINT: &str = "BTC__RPC__HTTP_ENDPOINT";
pub const BTC_RPC_USER: &str = "BTC__RPC__BASIC_AUTH_USER";
pub const BTC_RPC_PASSWORD: &str = "BTC__RPC__BASIC_AUTH_PASSWORD";

pub const BTC_BACKUP_HTTP_ENDPOINT: &str = "BTC__BACKUP_RPC__HTTP_ENDPOINT";
pub const BTC_BACKUP_RPC_USER: &str = "BTC__BACKUP_RPC__BASIC_AUTH_USER";
pub const BTC_BACKUP_RPC_PASSWORD: &str = "BTC__BACKUP_RPC__BASIC_AUTH_PASSWORD";

pub const DOT_WS_ENDPOINT: &str = "DOT__RPC__WS_ENDPOINT";
pub const DOT_HTTP_ENDPOINT: &str = "DOT__RPC__HTTP_ENDPOINT";

pub const DOT_BACKUP_WS_ENDPOINT: &str = "DOT__BACKUP_RPC__WS_ENDPOINT";
pub const DOT_BACKUP_HTTP_ENDPOINT: &str = "DOT__BACKUP_RPC__HTTP_ENDPOINT";

/// IP Address and port on which we listen for incoming p2p connections
pub const NODE_P2P_IP_ADDRESS: &str = "NODE_P2P__IP_ADDRESS";
pub const NODE_P2P_PORT: &str = "NODE_P2P__PORT";

/// Base path for all files
pub const CONFIG_ROOT: &str = "CF_CONFIG_ROOT";
pub const DEFAULT_CONFIG_ROOT: &str = "/etc/chainflip";

/// Lifetime in blocks of submitted signed extrinsics
pub const SIGNED_EXTRINSIC_LIFETIME: state_chain_runtime::BlockNumber = 128;
