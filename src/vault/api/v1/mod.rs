use crate::{
    common::api::{self, using, ResponseError},
    side_chain::{ISideChain, SideChainTx},
    vault::transactions::TransactionProvider,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::Filter;

#[cfg(test)]
mod tests;

/// The v1 API endpoints
pub fn endpoints<S: ISideChain + Send, T: TransactionProvider + Send>(
    side_chain: Arc<Mutex<S>>,
    provider: Arc<Mutex<T>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let blocks = warp::path!("blocks")
        .and(warp::get())
        .and(warp::query::<BlocksQueryParams>())
        .and(using(side_chain.clone()))
        .map(get_blocks)
        .and_then(api::respond);

    let quote = warp::path!("quote")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(side_chain.clone()))
        .and(using(provider.clone()))
        .map(post_quote)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(blocks.or(quote))
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

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockResponseEntry {
    id: u32,
    transactions: Vec<SideChainTx>,
}

/// Typed representation of the response for /blocks
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryResponse {
    total_blocks: u32,
    blocks: Vec<BlockResponseEntry>,
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

        let transactions = block
            .txs
            .iter()
            .map(|tx| tx.clone().into())
            .collect::<Vec<_>>();

        let block = BlockResponseEntry {
            id: block.id,
            transactions,
        };
        blocks.push(block);
    }

    let res = BlocksQueryResponse {
        total_blocks,
        blocks,
    };

    Ok(res)
}

// ==============================

#[serde(rename_all = "camelCase")]
#[derive(Debug, Serialize, Deserialize)]
pub struct QuoteParams {
    input_coin: String,
    input_return_address: String,
    input_address_id: String,
    input_amount: String, // Amounts are strings,
    output_coin: String,
    output_address: String,
    slippage_limit: f64,
}

#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct QuoteResponse {
    id: String,      // unique id
    created_at: u64, // milliseconds from epoch
    expires_at: u64, // milliseconds from epoch
    input_coin: String,
    input_address: String,        // Generated on the server,
    input_return_address: String, // User specified address,
    input_amount: String,
    output_coin: String,
    output_address: String,
    estimated_output_amount: String, // Generated on the server. Quoted amount.
    slippage_limit: f64,
}

pub async fn post_quote<S: ISideChain, T: TransactionProvider>(
    params: QuoteParams,
    side_chain: Arc<Mutex<S>>,
    provider: Arc<Mutex<T>>,
) -> Result<QuoteResponse, ResponseError> {
    todo!()
}
