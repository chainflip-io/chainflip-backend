use std::sync::Arc;

use crate::{
    common::{api::ResponseError, Coin, PoolCoin, StakerId},
    vault::transactions::TransactionProvider,
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Parameters for GET /v1/portions request
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct PortionsParams {
    pub staker_id: String,
    pub pool: Coin,
}

#[serde(rename_all = "camelCase")]
#[derive(Debug, Serialize)]
pub struct PortionsResponse {
    portions: String,
}

/// Get portions for staker id
///
/// # Example Query
///
/// > GET /v1/portions?staker_id=0433829aa2cccda485ee215421bd6c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293&pool=ETH
pub async fn get_portions<T>(
    params: PortionsParams,
    provider: Arc<RwLock<T>>,
) -> Result<PortionsResponse, ResponseError>
where
    T: TransactionProvider,
{
    let staker_id = StakerId::new(params.staker_id)
        .map_err(|err| bad_request!("Invalid staker id: {}", err))?;

    let pool = PoolCoin::from(params.pool).map_err(|err| bad_request!("Invalid pool: {}", err))?;

    let provider = provider.read();

    let all_pools = provider.get_portions();

    let pool = all_pools
        .get(&pool)
        .ok_or(bad_request!("No portions for pool {}", pool))?;

    let portions = pool
        .get(&staker_id)
        .ok_or(bad_request!("No portions for staker {}", staker_id))?;

    Ok(PortionsResponse {
        portions: portions.0.to_string(),
    })
}
