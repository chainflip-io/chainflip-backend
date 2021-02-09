use crate::{
    common::{api::ResponseError, *},
    vault::transactions::TransactionProvider,
};
use chainflip_common::{
    types::{
        chain::{Validate, WithdrawRequest},
        coin::Coin,
        fraction::WithdrawFraction,
        unique_id::GetUniqueId,
        Timestamp,
    },
    validation::validate_address,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{str::FromStr, sync::Arc};
use warp::http::StatusCode;

use super::Config;

/// Request parameters for withdraw
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawParams {
    /// Staker's public key
    staker_id: String,
    /// Pool to withdraw from
    pool: Coin,
    /// Return base address
    base_address: String,
    /// Return `PoolCoin` address
    other_address: String,
    /// Request creation timestamp
    timestamp: String,
    /// Percentage of the total portions to withdraw
    fraction: u32,
    /// Signature over the above fields
    signature: String,
}

fn check_staker_id_exists<T: TransactionProvider>(
    staker_id: &StakerId,
    pool: PoolCoin,
    provider: &Arc<RwLock<T>>,
) -> bool {
    let provider = provider.read();

    let all_pools = provider.get_portions();

    let pool_portions = match all_pools.get(&pool) {
        Some(v) => v,
        None => {
            return false;
        }
    };

    pool_portions.contains_key(&staker_id)
}

/// Handle withdraw request after initial parameter parsing
pub async fn post_withdraw<T: TransactionProvider>(
    params: WithdrawParams,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> Result<serde_json::Value, ResponseError> {
    // Check staker Id:
    let staker_id = StakerId::new(params.staker_id)
        .map_err(|err| bad_request!("Invalid staker id: {}", err))?;

    let pool = PoolCoin::from(params.pool).map_err(|err| bad_request!("Invalid pool: {}", err))?;

    // Check base address
    validate_address(Coin::BASE_COIN, config.net_type, &params.base_address)
        .map_err(|err| bad_request!("Invalid base address: {}", err))?;

    // Check the other address
    validate_address(pool.into(), config.net_type, &params.other_address)
        .map_err(|err| bad_request!("Invalid address for coin {}: {}", pool, err))?;

    // We don't want to pollute our db with invalid transactions
    if !check_staker_id_exists(&staker_id, pool, &provider) {
        return Err(bad_request!("Unknown staker id"));
    }

    // Check fraction (currently we only allow full withdrawing, i.e. WithdrawFraction::MAX)
    let fraction = WithdrawFraction::new(params.fraction)
        .map_err(|err| bad_request!("Invalid percentage: {}", err))?;

    if fraction != WithdrawFraction::MAX {
        return Err(bad_request!(
            "Fraction must be 10000 (partial unstaking is not yet supported)"
        ));
    }

    let signature =
        base64::decode(params.signature).map_err(|_| bad_request!("Invalid base64 signature"))?;

    let timestamp =
        Timestamp::from_str(&params.timestamp).map_err(|err| bad_request!("{}", err))?;

    let mut provider = provider.write();

    let tx = WithdrawRequest {
        timestamp,
        staker_id: staker_id.bytes().to_vec(),
        pool: pool.get_coin(),
        base_address: params.base_address.into(),
        other_address: params.other_address.into(),
        fraction,
        signature,
        event_number: None,
    };

    tx.validate(config.net_type)
        .map_err(|err| bad_request!("{}", err))?;

    let tx_id = tx.unique_id();

    provider.add_local_events(vec![tx.into()]).map_err(|_| {
        ResponseError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not record withdraw request transaction",
        )
    })?;

    let json = serde_json::json!({ "id": tx_id });

    Ok(json)
}
