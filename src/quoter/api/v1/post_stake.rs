use crate::{
    common::{api::ResponseError, coins::Coin, PoolCoin},
    quoter::vault_node::{StakeQuoteParams, VaultNodeInterface},
};
use rand::{prelude::StdRng, Rng, SeedableRng};
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
pub struct PostStakeParams {
    /// The input coin
    pub pool: String,
    /// The staker id
    pub staker_id: String,
    /// Address to return Loki to if Stake quote already fulfilled
    pub loki_return_address: String,
    /// Address to return other coin to if Stake quote already fulfilled
    pub other_return_address: String,
}

/// Submit a stake quoter
pub async fn stake<V: VaultNodeInterface>(
    params: PostStakeParams,
    vault_node: Arc<V>,
    input_id_cache: Arc<Mutex<HashMap<Coin, BTreeSet<String>>>>,
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

    let quote_params = StakeQuoteParams {
        pool: coin,
        staker_id: params.staker_id,
        coin_input_address_id: coin_input_address_id.clone(),
        loki_input_address_id: loki_input_address_id.clone(),
        loki_return_address: params.loki_return_address,
        other_return_address: params.other_return_address,
    };

    match vault_node.submit_stake(quote_params).await {
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
