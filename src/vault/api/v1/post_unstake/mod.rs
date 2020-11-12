use std::{str::FromStr, sync::Arc};

use serde::{Deserialize, Serialize};
use warp::http::StatusCode;

use parking_lot::RwLock;

use crate::{
    common::{api::ResponseError, *},
    transactions::UnstakeRequestTx,
    utils::validation::validate_address,
    vault::transactions::TransactionProvider,
};

/// Request parameters for unstake
#[derive(Deserialize, Serialize)]
pub struct UnstakeParams {
    /// Staker's public key
    staker_id: String,
    /// Pool to unstake from
    pool: Coin,
    /// Return Loki address
    loki_address: String,
    /// Return `PoolCoin` address
    other_address: String,
    /// Request creation timestamp
    timestamp: String,
    /// Percentage of the total portions to unstake
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

/// Handle unstake request after initial parameter parsing
pub async fn post_unstake<T: TransactionProvider>(
    params: UnstakeParams,
    provider: Arc<RwLock<T>>,
) -> Result<serde_json::Value, ResponseError> {
    // Check staker Id:
    let staker_id = StakerId::new(params.staker_id)
        .map_err(|err| bad_request!("Invalid staker id: {}", err))?;

    let pool = PoolCoin::from(params.pool).map_err(|err| bad_request!("Invalid pool: {}", err))?;

    // Check loki address
    let loki_address = LokiWalletAddress::from_str(&params.loki_address)
        .map_err(|_| bad_request!("Invalid Loki address"))?;

    // Check the other address
    validate_address(pool.into(), &params.other_address)
        .map_err(|err| bad_request!("Invalid address for coin {}: {}", pool, err))?;

    let other_address = WalletAddress(params.other_address);

    // We don't want to pollute our db with invalid transactions
    if !check_staker_id_exists(&staker_id, pool, &provider) {
        return Err(bad_request!("Unknown staker id"));
    }

    // Check fraction (currently we only allow full unstaking, i.e. UnstakeFraction::MAX)
    let fraction = UnstakeFraction::new(params.fraction)
        .map_err(|err| bad_request!("Invalid percentage: {}", err))?;

    if fraction != UnstakeFraction::MAX {
        return Err(bad_request!(
            "Fraction must be 10000 (partial unstaking is not yet supported)"
        ));
    }

    let timestamp =
        Timestamp::from_str(&params.timestamp).map_err(|err| bad_request!("{}", err))?;

    let mut provider = provider.write();

    let tx = UnstakeRequestTx::new(
        pool,
        staker_id,
        loki_address.into(),
        other_address,
        fraction,
        timestamp,
        params.signature,
    );

    tx.verify().map_err(|_| bad_request!("Invalid signature"))?;

    let tx_id = tx.id;

    provider.add_transactions(vec![tx.into()]).map_err(|_| {
        ResponseError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not record unstake transaction",
        )
    })?;

    let json = serde_json::json!({ "id": tx_id });

    Ok(json)
}
