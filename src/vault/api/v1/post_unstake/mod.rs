use std::{convert::TryFrom, str::FromStr, sync::Arc};

use serde::Deserialize;
use warp::http::StatusCode;

use parking_lot::RwLock;

use crate::{
    common::{api::ResponseError, LokiWalletAddress, PoolCoin, StakerId, Timestamp, WalletAddress},
    transactions::UnstakeRequestTx,
    vault::transactions::TransactionProvider,
};

/// Request parameters for unstake
#[derive(Deserialize)]
pub struct UnstakeParams {
    /// Staker's public key
    staker_id: StakerId,
    /// Pool to unstake from
    pool: PoolCoin,
    /// Return Loki address
    loki_address: WalletAddress,
    /// Return `PoolCoin` address
    other_address: WalletAddress,
    /// Request creation timestamp
    timestamp: String,
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
    // Check loki address
    let loki_address = LokiWalletAddress::try_from(params.loki_address)
        .map_err(|_| ResponseError::new(StatusCode::BAD_REQUEST, "Invalid Loki address"))?;

    // We don't want to pollute our db with invalid transactions
    if !check_staker_id_exists(&params.staker_id, params.pool, &provider) {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Unknown staker id",
        ));
    }

    let timestamp = Timestamp::from_str(&params.timestamp)
        .map_err(|err| ResponseError::new(StatusCode::BAD_REQUEST, err))?;

    // Check signature here? (It will be checked by processor)

    let mut provider = provider.write();

    let tx = UnstakeRequestTx::new(
        params.pool,
        params.staker_id,
        loki_address.into(),
        params.other_address,
        timestamp,
        params.signature,
    );

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
