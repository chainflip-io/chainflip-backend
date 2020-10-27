use crate::{
    common::api::ResponseError,
    quoter::StateProvider,
    vault::{api::v1::post_quote::SwapQuoteResponse, processor::utils::get_swap_expire_timestamp},
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
    // TODO: Also return stake quotes
    let id = match Uuid::from_str(&params.id) {
        Ok(id) => id,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid quote id",
            ))
        }
    };

    let quote = state.lock().unwrap().get_swap_quote_tx(id);
    let response = match quote {
        Some(quote) => {
            let response = SwapQuoteResponse {
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
            };
            Some(GetQuoteResponse::Swap(response))
        }
        _ => None,
    };

    Ok(response)
}
