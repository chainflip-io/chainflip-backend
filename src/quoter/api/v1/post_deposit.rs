use crate::{
    common::{api::ResponseError, input_address_id::input_address_id_to_string, PoolCoin},
    quoter::vault_node::VaultNodeInterface,
    vault::api::v1::post_deposit::DepositQuoteParams,
};
use chainflip_common::types::coin::Coin;
use rand::{prelude::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use warp::http::StatusCode;

use super::{utils::generate_unique_input_address_id, InputIdCache};

/// Parameters for POST `quote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostDepositParams {
    /// The input coin
    pub pool: String,
    /// The staker id
    pub staker_id: String,
    /// Address to return Loki to if Stake quote already fulfilled
    pub loki_return_address: String,
    /// Address to return other coin to if Stake quote already fulfilled
    pub other_return_address: String,
}

/// TODO: Rename this to deposit
/// Submit a deposit quote
pub async fn deposit<V: VaultNodeInterface>(
    params: PostDepositParams,
    vault_node: Arc<V>,
    input_id_cache: Arc<Mutex<InputIdCache>>,
) -> Result<serde_json::Value, ResponseError> {
    let coin = Coin::from_str(&params.pool)
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid pool coin"))?;

    if let Err(_) = PoolCoin::from(coin) {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Invalid pool coin",
        ));
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Duration since UNIX_EPOCH failed");
    let mut rng = StdRng::seed_from_u64(now.as_secs());
    let coin_input_address_id =
        generate_unique_input_address_id(coin, input_id_cache.clone(), &mut rng)?;
    let loki_input_address_id =
        generate_unique_input_address_id(Coin::LOKI, input_id_cache.clone(), &mut rng)?;

    // Convert to string representation
    let string_coin_input_address_id =
        input_address_id_to_string(coin, &coin_input_address_id).expect("Invalid input address id");
    let string_loki_input_address_id =
        input_address_id_to_string(Coin::LOKI, &loki_input_address_id)
            .expect("Invalid input address id");

    let quote_params = DepositQuoteParams {
        pool: coin,
        staker_id: params.staker_id,
        coin_input_address_id: string_coin_input_address_id,
        loki_input_address_id: string_loki_input_address_id,
        loki_return_address: params.loki_return_address,
        other_return_address: params.other_return_address,
    };

    match vault_node.submit_deposit(quote_params).await {
        Ok(result) => Ok(result),
        Err(err) => {
            // Something went wrong, remove id from cache
            let mut cache = input_id_cache.lock().unwrap();
            cache.get_mut(&coin).unwrap().remove(&coin_input_address_id);
            cache
                .get_mut(&Coin::LOKI)
                .unwrap()
                .remove(&loki_input_address_id);
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                &format!("{}", err),
            ));
        }
    }
}
