use super::input_id_cache::InputIdCache;
use crate::{
    common::{api::ResponseError, PoolCoin},
    quoter::vault_node::VaultNodeInterface,
    vault::api::v1::post_deposit::DepositQuoteParams,
};
use chainflip_common::{types::coin::Coin, utils::address_id};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, sync::Arc};
use warp::http::StatusCode;

/// Parameters for POST `quote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostDepositParams {
    /// The input coin
    pub pool: String,
    /// The staker id
    pub staker_id: String,
    /// Address to return the base coin to if Stake quote already fulfilled
    pub base_return_address: String,
    /// Address to return other coin to if Stake quote already fulfilled
    pub other_return_address: String,
}

/// Submit a deposit quote
pub async fn deposit<V: VaultNodeInterface>(
    params: PostDepositParams,
    vault_node: Arc<V>,
    input_id_cache: InputIdCache,
) -> Result<serde_json::Value, ResponseError> {
    let coin = Coin::from_str(&params.pool)
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid pool coin"))?;

    if PoolCoin::from(coin).is_err() {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Invalid pool coin",
        ));
    };

    let coin_input_address_id = input_id_cache.generate_unique_input_address_id(&coin);
    let base_input_address_id = input_id_cache.generate_unique_input_address_id(&Coin::BASE_COIN);

    // Convert to string representation
    let string_coin_input_address_id =
        address_id::to_string(coin, &coin_input_address_id).expect("Invalid input address id");
    let string_base_input_address_id =
        address_id::to_string(Coin::BASE_COIN, &base_input_address_id)
            .expect("Invalid input address id");

    let quote_params = DepositQuoteParams {
        pool: coin,
        staker_id: params.staker_id,
        other_input_address_id: string_coin_input_address_id,
        base_input_address_id: string_base_input_address_id,
        base_return_address: params.base_return_address,
        other_return_address: params.other_return_address,
    };

    match vault_node.submit_deposit(quote_params).await {
        Ok(result) => Ok(result),
        Err(err) => {
            // Something went wrong, remove ids from cache
            input_id_cache.remove(&coin, &coin_input_address_id);
            input_id_cache.remove(&Coin::BASE_COIN, &base_input_address_id);

            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                &format!("{}", err),
            ));
        }
    }
}
