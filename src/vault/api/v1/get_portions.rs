use std::{convert::TryInto, sync::Arc};

use crate::{
    common::{api::ResponseError, Coin, PoolCoin, StakerId},
    vault::transactions::{memory_provider::Portion, TransactionProvider},
};

use crate::utils::primitives::U256;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Parameters for GET /v1/portions request
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct PortionsParams {
    /// Get portions associated with this staker Id
    pub staker_id: String,
    /// Pool identified by coin type other than Loki
    pub pool: Coin,
}

#[serde(rename_all = "camelCase")]
#[derive(Debug, Serialize)]
pub struct PortionsResponse {
    portions: String,
    estimated_loki_amount: String,
    estimated_other_amount: String,
}

fn portion_of_total(total: u128, portions: Portion) -> u128 {
    let total: U256 = total.into();

    let portions: U256 = portions.0.into();

    let max_portions: U256 = Portion::MAX.0.into();

    let amount = total * portions / max_portions;

    amount.try_into().expect("Unexpected overflow")
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

    let pool_coin =
        PoolCoin::from(params.pool).map_err(|err| bad_request!("Invalid pool: {}", err))?;

    let provider = provider.read();

    let all_pools = provider.get_portions();

    let pool = all_pools
        .get(&pool_coin)
        .ok_or(bad_request!("No portions for pool {}", pool_coin))?;

    let portions = *pool.get(&staker_id).unwrap_or(&Portion(0));

    let liquidity = provider
        .get_liquidity(pool_coin)
        .ok_or(internal_error!("Unexpected missing liquidity"))?;

    let loki = portion_of_total(liquidity.loki_depth, portions);
    let other = portion_of_total(liquidity.loki_depth, portions);

    let loki = loki.to_string();
    let other = other.to_string();

    Ok(PortionsResponse {
        portions: portions.0.to_string(),
        estimated_loki_amount: loki,
        estimated_other_amount: other,
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn check_portions_of_total_amount() {
        assert_eq!(portion_of_total(1000, Portion::MAX), 1000);
        assert_eq!(portion_of_total(u128::MAX, Portion::MAX), u128::MAX);

        let portions = Portion(Portion::MAX.0 * 3 / 4);

        assert_eq!(portion_of_total(1000, portions), 750);
    }
}
