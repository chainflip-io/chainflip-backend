use crate::{
    common::{api::ResponseError, coins::Coin},
    quoter::{vault_node::QuoteParams, vault_node::VaultNodeInterface},
};
use rand::{prelude::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    str::FromStr,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use warp::http::StatusCode;

use super::utils::generate_unique_input_address_id;

/// Parameters for POST `quote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostQuoteParams {
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

/// Submit a swap quote
pub async fn swap<V: VaultNodeInterface>(
    params: PostQuoteParams,
    vault_node: Arc<V>,
    input_id_cache: Arc<Mutex<HashMap<Coin, BTreeSet<String>>>>,
) -> Result<serde_json::Value, ResponseError> {
    let input_coin = match Coin::from_str(&params.input_coin) {
        Ok(coin) => coin,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid input coin",
            ))
        }
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Duration since UNIX_EPOCH failed");
    let rng = StdRng::seed_from_u64(now.as_secs());
    let input_address_id =
        generate_unique_input_address_id(input_coin, input_id_cache.clone(), rng)?;

    let quote_params = QuoteParams {
        input_coin: params.input_coin,
        input_amount: params.input_amount,
        input_address_id: input_address_id.clone(),
        input_return_address: params.input_return_address,
        output_address: params.output_address,
        slippage_limit: params.slippage_limit,
    };

    match vault_node.submit_quote(quote_params).await {
        Ok(result) => Ok(result),
        Err(err) => {
            // Something went wrong, remove id from cache
            input_id_cache
                .lock()
                .unwrap()
                .get_mut(&input_coin)
                .unwrap()
                .remove(&input_address_id);
            return Err(ResponseError::new(StatusCode::BAD_REQUEST, &err));
        }
    }
}
