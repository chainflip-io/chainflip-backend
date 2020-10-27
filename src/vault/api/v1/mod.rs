use crate::{
    common::api::{self, using, ResponseError},
    side_chain::{ISideChain, SideChainBlock},
    vault::config::NetType,
    vault::{config::VaultConfig, config::VAULT_CONFIG, transactions::TransactionProvider},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::Filter;

#[cfg(test)]
mod tests;

/// Post swap quote endpoint
pub mod post_swap;

/// Post stake quote endpoint
pub mod post_stake;

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

    let stake = warp::path!("swap")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_stake::stake)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(blocks.or(swap).or(stake))
}

// ==============================

/// Parameters for GET /v1/blocks request
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryParams {
    number: Option<u32>,
    limit: Option<u32>,
}

impl BlocksQueryParams {
    /// Construct params from values
    pub fn new(number: u32, limit: u32) -> Self {
        BlocksQueryParams {
            number: Some(number),
            limit: Some(limit),
        }
    }
}

/// Typed representation of the response for /blocks
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryResponse {
    pub total_blocks: u32,
    pub blocks: Vec<SideChainBlock>,
}

const DEFAULT_BLOCK_NUMBER: u32 = 0;
/// Clients can request up to this number of blocks in one request
const MAX_BLOCKS_IN_RESPONSE: u32 = 50;

/// Get the side chain blocks
///
/// # Example Query
///
/// > GET /v1/blocks?number=0&limit=50
pub async fn get_blocks<S: ISideChain>(
    params: BlocksQueryParams,
    side_chain: Arc<Mutex<S>>,
) -> Result<BlocksQueryResponse, ResponseError> {
    let BlocksQueryParams { number, limit } = params;

    let number = number.unwrap_or(DEFAULT_BLOCK_NUMBER);
    let limit = limit.unwrap_or(MAX_BLOCKS_IN_RESPONSE);

    let side_chain = side_chain.lock().unwrap();
    let total_blocks = side_chain.total_blocks();

    if total_blocks == 0 || number >= total_blocks || limit == 0 {
        // Return an empty response
        let res = BlocksQueryResponse {
            total_blocks,
            blocks: vec![],
        };
        return Ok(res);
    }

    let limit = std::cmp::min(limit, MAX_BLOCKS_IN_RESPONSE);

    let last_valid_idx = total_blocks.saturating_sub(1);

    let last_requested_idx = number.saturating_add(limit).saturating_sub(1);

    let last_idx = std::cmp::min(last_valid_idx, last_requested_idx);

    let mut blocks = Vec::with_capacity(limit as usize);

    // TODO: optimise this for a range of blocks?
    for idx in number..=last_idx {
        // We already checked the boundaries, so just asserting here:
        let block = side_chain.get_block(idx).expect("Failed to get block");
        blocks.push(block.clone());
    }

    let res = BlocksQueryResponse {
        total_blocks,
        blocks,
    };

    Ok(res)
}
