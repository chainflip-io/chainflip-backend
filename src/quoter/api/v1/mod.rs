use crate::{
    common::{
        api,
        api::{using, ResponseError},
        coins::{Coin, CoinInfo},
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
};
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use warp::http::StatusCode;
use warp::Filter;

#[cfg(test)]
mod test;

/// The v1 API endpoints
pub fn endpoints<V, S>(
    _vault_node: Arc<V>,
    state: Arc<Mutex<S>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
where
    V: VaultNodeInterface,
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

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(coins.or(estimate)) // .and(coins.or(another).or(yet_another))
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
        return Ok(Coin::ALL.iter().map(|coin| coin.get_info()).collect());
    }

    // Filter out invalid coins
    let valid_coins: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .collect();

    let info = valid_coins.iter().map(|coin| coin.get_info()).collect();

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
