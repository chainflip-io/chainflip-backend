use crate::{
    common::{api::ResponseError, StakerId},
    quoter::StateProvider,
    vault::api::v1::post_deposit::DepositQuoteResponse,
    vault::{api::v1::post_swap::SwapQuoteResponse, processor::utils::get_swap_expire_timestamp},
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use chainflip_common::types::{chain::UniqueId, unique_id::GetUniqueId};

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
    Deposit(DepositQuoteResponse),
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
    let id = match params.id.parse::<u64>() {
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

    if let Some(response) = get_deposit_quote(id, state.clone()) {
        return Ok(Some(GetQuoteResponse::Deposit(response)));
    }

    Ok(None)
}

pub fn get_swap_quote<S>(id: UniqueId, state: Arc<Mutex<S>>) -> Option<SwapQuoteResponse>
where
    S: StateProvider,
{
    let quote = state.lock().unwrap().get_swap_quote(id);
    quote.map(|quote| SwapQuoteResponse {
        id: quote.unique_id(),
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        input_coin: quote.input,
        input_address: quote.input_address.to_string(),
        input_return_address: quote.return_address.map(|r| r.to_string()),
        effective_price: quote.effective_price,
        output_coin: quote.output,
        output_address: quote.output_address.to_string(),
        slippage_limit: quote.slippage_limit.map_or(0, |fraction| fraction.value()),
    })
}

pub fn get_deposit_quote<S>(id: UniqueId, state: Arc<Mutex<S>>) -> Option<DepositQuoteResponse>
where
    S: StateProvider,
{
    let quote = state.lock().unwrap().get_deposit_quote(id);
    quote.map(|quote| DepositQuoteResponse {
        created_at: quote.timestamp.0,
        expires_at: get_swap_expire_timestamp(&quote.timestamp).0,
        pool: quote.pool,
        staker_id: StakerId::from_bytes(&quote.staker_id).unwrap().to_string(),
        loki_input_address: quote.base_input_address.to_string(),
        coin_input_address: quote.coin_input_address.to_string(),
        loki_return_address: quote.base_return_address.to_string(),
        coin_return_address: quote.coin_return_address.to_string(),
    })
}
