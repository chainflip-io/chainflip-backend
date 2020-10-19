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

#[cfg(test)]
mod test {
    use super::*;
    use crate::quoter::api::v1::test::setup_memory_db;

    #[tokio::test]
    pub async fn get_estimate_validates_params() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        // =============

        let invalid_input_coin = EstimateParams {
            input_coin: "invalid".to_owned(),
            input_amount: 1000000,
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_coin, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid input coin");

        // =============

        let invalid_output_coin = EstimateParams {
            input_coin: "loki".to_owned(),
            input_amount: 1000000,
            output_coin: "invalid".to_owned(),
        };

        let error = get_estimate(invalid_output_coin, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid output coin");

        // =============

        let same_coins = EstimateParams {
            input_coin: "loki".to_owned(),
            input_amount: 1000000,
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(same_coins, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(
            error.message,
            "Input coin must be different from output coin"
        );

        // =============

        let invalid_input_amount = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: 0,
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_amount, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Input amount must be greater than 0");

        // =============

        let valid_params = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: 100,
            output_coin: "loki".to_owned(),
        };

        get_estimate(valid_params, state.clone())
            .await
            .expect("Expected a valid result");
    }
}
