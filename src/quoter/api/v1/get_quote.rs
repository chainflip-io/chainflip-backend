use crate::{
    common::api::ResponseError,
    quoter::StateProvider,
    vault::api::v1::post_stake::StakeQuoteResponse,
    vault::{api::v1::post_swap::SwapQuoteResponse, processor::utils::get_swap_expire_timestamp},
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

/// Parameters for GET `quote` endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetQuoteParams {
    /// The quote id
    pub id: String,
}

/// Response for GET `quote` endpoint
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "info")]
pub enum GetQuoteResponse {
    Swap(SwapQuoteResponse),
    Stake(StakeQuoteResponse),
}

/// Get information about a quote
///
/// # Example Query
///
/// > GET /v1/quote?id=<quote-id>
pub async fn get_quote<S>(
    params: GetQuoteParams,
    state: Arc<Mutex<S>>,
) -> Result<Option<GetQuoteResponse>, ResponseError>
where
    S: StateProvider,
{
    let id = match Uuid::from_str(&params.id) {
        Ok(id) => id,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid quote id",
            ))
        }
    };

    if let Some(response) = get_swap_quote(id, state.clone()) {
        return Ok(Some(GetQuoteResponse::Swap(response)));
    }

    if let Some(response) = get_stake_quote(id, state.clone()) {
        return Ok(Some(GetQuoteResponse::Stake(response)));
    }

    Ok(None)
}

pub fn get_swap_quote<S>(id: Uuid, state: Arc<Mutex<S>>) -> Option<SwapQuoteResponse>
where
    S: StateProvider,
{
    let quote = state.lock().unwrap().get_swap_quote_tx(id);
    quote.map(|quote| SwapQuoteResponse {
        id: quote.id,
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        input_coin: quote.input,
        input_address: quote.input_address.to_string(),
        input_return_address: quote.return_address.map(|r| r.to_string()),
        effective_price: quote.effective_price,
        output_coin: quote.output,
        output_address: quote.output_address.to_string(),
        slippage_limit: quote.slippage_limit,
    })
}

pub fn get_stake_quote<S>(id: Uuid, state: Arc<Mutex<S>>) -> Option<StakeQuoteResponse>
where
    S: StateProvider,
{
    let quote = state.lock().unwrap().get_stake_quote_tx(id);
    quote.map(|quote| StakeQuoteResponse {
        id: quote.id,
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        pool: quote.coin_type.get_coin(),
        staker_id: quote.staker_id.inner().to_owned(),
        loki_input_address: quote.loki_input_address.0,
        coin_input_address: quote.coin_input_address.0,
    })
}
