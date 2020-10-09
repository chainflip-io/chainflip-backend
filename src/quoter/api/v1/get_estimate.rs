use crate::{
    common::{api::ResponseError, coins::Coin},
    quoter::StateProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use warp::http::StatusCode;

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
