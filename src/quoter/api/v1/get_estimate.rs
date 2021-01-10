use crate::{common::api::ResponseError, quoter::StateProvider, utils::price};
use chainflip_common::types::coin::Coin;
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
    pub input_amount: String,
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
/// > GET /v1/estimate?inputCoin=LOKI&inputAmount=1000000000&outputCoin=BTC
pub async fn get_estimate<S>(
    params: EstimateParams,
    state: Arc<Mutex<S>>,
) -> Result<EstimateResponse, ResponseError>
where
    S: StateProvider,
{
    let input_coin = Coin::from_str(&params.input_coin)
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid input coin"))?;

    let input_amount = params
        .input_amount
        .parse::<u128>()
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid input amount"))?;

    let output_coin = Coin::from_str(&params.output_coin)
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid output coin"))?;

    if input_coin == output_coin {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Input coin must be different from output coin",
        ));
    }

    if input_amount == 0 {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Input amount must be greater than 0",
        ));
    }

    let calculation = {
        let state = state.lock().unwrap();

        match price::get_output(&*state, input_coin, input_amount, output_coin) {
            Ok(calculation) => calculation,
            Err(err) => {
                error!(
                    "Failed to calculate output for params: {:?}. {}",
                    params, err
                );
                return Err(ResponseError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to calculate output",
                ));
            }
        }
    };

    let price = calculation.second.unwrap_or(calculation.first);

    Ok(EstimateResponse {
        output_amount: price.output_amount.to_string(),
        loki_fee: price.loki_fee.to_string(),
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::quoter::api::v1::test::setup_memory_db;

    #[tokio::test]
    pub async fn get_estimate_returns_invalid_input_coin() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let invalid_input_coin = EstimateParams {
            input_coin: "invalid".to_owned(),
            input_amount: "1000000".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_coin, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid input coin");
    }

    #[tokio::test]
    pub async fn get_estimate_returns_invalid_output_coin() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let invalid_output_coin = EstimateParams {
            input_coin: "loki".to_owned(),
            input_amount: "1000000".to_owned(),
            output_coin: "invalid".to_owned(),
        };

        let error = get_estimate(invalid_output_coin, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid output coin");
    }

    #[tokio::test]
    pub async fn get_estimate_returns_invalid_input_amount() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let invalid_input_amount = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: "-123".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_amount, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid input amount");

        let invalid_input_amount = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: "abc".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_amount, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid input amount");

        let invalid_input_amount = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: "123.0123".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_amount, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Invalid input amount");
    }

    #[tokio::test]
    pub async fn get_estimate_returns_coins_must_be_different() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let same_coins = EstimateParams {
            input_coin: "loki".to_owned(),
            input_amount: "1000000".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(same_coins, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(
            error.message,
            "Input coin must be different from output coin"
        );
    }

    #[tokio::test]
    pub async fn get_estimate_returns_input_amount_gt_0() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let invalid_input_amount = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: "0".to_owned(),
            output_coin: "loki".to_owned(),
        };

        let error = get_estimate(invalid_input_amount, state.clone())
            .await
            .expect_err("Expected an error");

        assert_eq!(error.message, "Input amount must be greater than 0");
    }

    #[tokio::test]
    pub async fn get_estimate_returns_valid_result() {
        let db = setup_memory_db();
        let state = Arc::new(Mutex::new(db));

        let valid_params = EstimateParams {
            input_coin: "btc".to_owned(),
            input_amount: "1000000".to_owned(),
            output_coin: "loki".to_owned(),
        };

        get_estimate(valid_params, state.clone())
            .await
            .expect("Expected a valid result");
    }
}
