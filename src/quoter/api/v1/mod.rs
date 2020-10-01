use crate::{
    common::{
        api,
        api::{using, ResponseError},
        coins::{Coin, CoinInfo},
        Timestamp,
    },
    quoter::{vault_node::QuoteParams, vault_node::VaultNodeInterface, StateProvider},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
};
use warp::http::StatusCode;
use warp::Filter;

#[cfg(test)]
mod test;

/// The v1 API endpoints
pub fn endpoints<V, S>(
    vault_node: Arc<V>,
    state: Arc<Mutex<S>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
where
    V: VaultNodeInterface + Send + Sync,
    S: StateProvider + Send,
{
    let coins = warp::path!("coins")
        .and(warp::get())
        .and(warp::query::<CoinsParams>())
        .map(get_coins)
        .and_then(api::respond);

    let estimate = warp::path!("estimate")
        .and(warp::get())
        .and(warp::query::<EstimateParams>())
        .and(using(state.clone()))
        .map(get_estimate)
        .and_then(api::respond);

    let pools = warp::path!("pools")
        .and(warp::get())
        .and(warp::query::<PoolsParams>())
        .and(using(state.clone()))
        .map(get_pools)
        .and_then(api::respond);

    let submit_quote = warp::path!("submitQuote")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(state.clone()))
        .and(using(vault_node.clone()))
        .map(submit_quote)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(coins.or(estimate).or(pools).or(submit_quote))
}

/// Parameters for `coins` endpoint
#[derive(Debug, Deserialize)]
pub struct CoinsParams {
    /// The list of coin symbols
    pub symbols: Option<Vec<String>>,
}

/// Get coins that we support.
///
/// If `symbols` is empty then all coins will be returned.
/// If `symbols` is not empty then only information for valid symbols will be returned.
///
/// # Example Query
///
/// > GET /v1/coins?symbols=BTC,loki
pub async fn get_coins(params: CoinsParams) -> Result<Vec<CoinInfo>, ResponseError> {
    // Return all coins if no params were passed
    if params.symbols.is_none() {
        return Ok(Coin::SUPPORTED.iter().map(|coin| coin.get_info()).collect());
    }

    // Filter out invalid symbols
    let valid_symbols: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .collect();

    let info = valid_symbols.iter().map(|coin| coin.get_info()).collect();

    return Ok(info);
}

/// Parameters for `estimate` endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateParams {
    /// The input coin symbol
    pub input_coin: String,
    /// The input amount in atomic value (actual * decimal)
    pub input_amount: u128,
    /// The output coin symbol
    pub output_coin: String,
}

/// Response for `estimate` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateResponse {
    /// The output amount in atomic value
    pub output_amount: String,
    /// The total loki fee
    pub loki_fee: String,
}

/// Get estimated output amount
///
/// # Example Query
///
/// > GET /v1/get_estimate?inputCoin=LOKI&inputAmount=1000000&outputCoin=btc
pub async fn get_estimate<S>(
    params: EstimateParams,
    _state: Arc<Mutex<S>>,
) -> Result<EstimateResponse, ResponseError>
where
    S: StateProvider,
{
    let input_coin = match Coin::from_str(&params.input_coin) {
        Ok(coin) => coin,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid input coin",
            ))
        }
    };

    let output_coin = match Coin::from_str(&params.output_coin) {
        Ok(coin) => coin,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid output coin",
            ))
        }
    };

    if input_coin == output_coin {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Input coin must be different from output coin",
        ));
    }

    if params.input_amount == 0 {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Input amount must be greater than 0",
        ));
    }

    // TODO: Add logic here

    Ok(EstimateResponse {
        output_amount: "0".to_owned(),
        loki_fee: "0".to_owned(),
    })
}

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

/// Response for `quote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {}

/// Parameters for `submitQuote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitQuoteParams {
    /// The input coin
    pub input_coin: String,
    /// The input amount
    pub input_amount: String,
    /// The input return address
    pub input_return_address: Option<String>,
    /// The output address
    pub output_address: String,
    /// The slippage limit
    pub slippage_limit: u32,
}

/// Submit a quote
pub async fn submit_quote<S, V>(
    params: SubmitQuoteParams,
    _state: Arc<Mutex<S>>,
    vault_node: Arc<V>,
) -> Result<QuoteResponse, ResponseError>
where
    S: StateProvider,
    V: VaultNodeInterface,
{
    // TODO: Add logic here
    let quote_params = QuoteParams {
        input_coin: params.input_coin,
        input_amount: params.input_amount,
        input_address_id: "0".to_owned(), // TODO: Populate this accordingly
        input_return_address: params.input_return_address,
        output_address: params.output_address,
        slippage_limit: params.slippage_limit,
    };

    match vault_node.submit_quote(quote_params) {
        Ok(_) => {}
        Err(err) => return Err(ResponseError::new(StatusCode::BAD_REQUEST, &err)),
    }

    return Ok(QuoteResponse {});
}
