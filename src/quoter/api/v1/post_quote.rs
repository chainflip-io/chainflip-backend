use crate::{
    common::{api::ResponseError, coins::Coin},
    quoter::{vault_node::QuoteParams, vault_node::VaultNodeInterface, StateProvider},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    str::FromStr,
    sync::{Arc, Mutex},
};
use warp::http::StatusCode;

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

/// Submit a quote
pub async fn quote<V, R>(
    params: PostQuoteParams,
    vault_node: Arc<V>,
    input_id_cache: Arc<Mutex<HashMap<Coin, BTreeSet<String>>>>,
    mut rng: R,
) -> Result<serde_json::Value, ResponseError>
where
    V: VaultNodeInterface,
    R: Rng,
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

    let mut cache = input_id_cache.lock().unwrap();
    let used_ids = cache.entry(input_coin).or_insert(BTreeSet::new()).clone();

    // How do we test this? :(
    let input_address_id = loop {
        let id = match input_coin {
            Coin::BTC => rng.gen_range(6, u64::MAX).to_string(),
            Coin::ETH => rng.gen_range(6, u64::MAX).to_string(),
            Coin::LOKI => {
                let random_bytes = rng.gen::<[u8; 8]>();
                hex::encode(random_bytes)
            }
            _ => {
                return Err(ResponseError::new(
                    StatusCode::BAD_REQUEST,
                    "Invalid input id",
                ))
            }
        };

        if !used_ids.contains(&id) {
            break id;
        }
    };

    let quote_params = QuoteParams {
        input_coin: params.input_coin,
        input_amount: params.input_amount,
        input_address_id: input_address_id.clone(),
        input_return_address: params.input_return_address,
        output_address: params.output_address,
        slippage_limit: params.slippage_limit,
    };

    // Add the id in the cache
    cache
        .get_mut(&input_coin)
        .unwrap()
        .insert(input_address_id.clone());

    drop(cache);

    match vault_node.submit_quote(quote_params) {
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
