use crate::{
    common::api::{self, using},
    side_chain::ISideChain,
    vault::config::NetType,
    vault::transactions::TransactionProvider,
};
use parking_lot::RwLock;
use std::sync::{Arc, Mutex};
use warp::Filter;

/// Post swap quote endpoint
pub mod post_swap;

/// Post stake quote endpoint
pub mod post_stake;

/// Post unstake endpoint
pub mod post_unstake;

/// Get blocks endpoint
pub mod get_blocks;
use get_blocks::{get_blocks, BlocksQueryParams};

/// Utils
pub mod utils;

#[derive(Debug, Clone)]
/// A config object for swap and stake
pub struct Config {
    /// Loki wallet address
    pub loki_wallet_address: String,
    /// Ethereum master root key
    pub eth_master_root_key: String,
    /// BTC master root key
    pub btc_master_root_key: String,
    /// Network type
    pub net_type: NetType,
}

/// The v1 API endpoints
pub fn endpoints<S: ISideChain + Send, T: TransactionProvider + Send + Sync>(
    side_chain: Arc<Mutex<S>>,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let blocks = warp::path!("blocks")
        .and(warp::get())
        .and(warp::query::<BlocksQueryParams>())
        .and(using(side_chain.clone()))
        .map(get_blocks)
        .and_then(api::respond);

    let swap = warp::path!("swap")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_swap::swap)
        .and_then(api::respond);

    let stake = warp::path!("stake")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_stake::stake)
        .and_then(api::respond);

    let unstake = warp::path!("unstake")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .map(post_unstake::post_unstake)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(blocks.or(swap).or(stake).or(unstake))
}
