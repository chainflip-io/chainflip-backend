use crate::{
    common::{api::ResponseError, coins::Coin, Timestamp},
    quoter::StateProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Parameters for `pools` endpoint
#[derive(Debug, Deserialize)]
pub struct PoolsParams {
    /// The list of coin symbols
    pub symbols: Option<Vec<String>>,
}

/// A representation of pool depth for `PoolsResponse`
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolDepth {
    /// The depth of one side of the pool in atomic units
    pub depth: String,
    /// The depth of the loki side of the pool in atomic units
    pub loki_depth: String,
}

/// Response for `pools` endpoint
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolsResponse {
    /// The timestamp of when the response was generated
    pub timestamp: Timestamp,
    /// A map of a coin and its pool depth
    pub pools: HashMap<Coin, PoolDepth>,
}

/// Get the current pools
///
/// If `symbols` is empty then all pools will be returned.
/// If `symbols` is not empty then only information for valid symbols will be returned.
///
/// # Example Query
///
/// > GET /v1/pools?symbols=BTC,eth
pub async fn get_pools<S>(
    params: PoolsParams,
    _state: Arc<Mutex<S>>,
) -> Result<PoolsResponse, ResponseError>
where
    S: StateProvider,
{
    // Return all pools if no params were passed
    if params.symbols.is_none() {
        return Ok(PoolsResponse {
            timestamp: Timestamp::now(),
            pools: HashMap::new(),
        });
    }

    // Filter out invalid symbols
    let _valid_symbols: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .filter(|symbol| symbol.clone() != Coin::LOKI)
        .collect();

    // TODO: Add logic here

    return Ok(PoolsResponse {
        timestamp: Timestamp::now(),
        pools: HashMap::new(),
    });
}
