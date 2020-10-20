use crate::{
    common::{api::ResponseError, coins::Coin},
    quoter::StateProvider,
    vault::processor::utils::get_swap_expire_timestamp,
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
#[serde(rename_all = "camelCase")]
#[derive(Debug, Serialize, Deserialize)]
pub struct QuoteResponse {
    id: Uuid,         // unique id
    created_at: u128, // milliseconds from epoch
    expires_at: u128, // milliseconds from epoch
    input_coin: Coin,
    input_address: String,                // Generated on the server,
    input_return_address: Option<String>, // User specified address,
    effective_price: f64,
    output_coin: Coin,
    output_address: String,
    slippage_limit: f32,
}

/// Get information about a quote
///
///
/// # Example Query
///
/// > GET /v1/quote?id=<quote-id>
pub async fn get_quote<S>(
    params: GetQuoteParams,
    state: Arc<Mutex<S>>,
) -> Result<Option<QuoteResponse>, ResponseError>
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

    let quote = state.lock().unwrap().get_swap_quote_tx(id);
    let response = match quote {
        Some(quote) => {
            let response = QuoteResponse {
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
            Some(response)
        }
        _ => None,
    };

    Ok(response)
}
