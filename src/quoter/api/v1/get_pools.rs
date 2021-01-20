use crate::{
    common::{api::ResponseError, Liquidity, PoolCoin},
    quoter::StateProvider,
};
use chainflip_common::types::{coin::Coin, Timestamp};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Parameters for `pools` endpoint
#[derive(Debug, Deserialize)]
pub struct PoolsParams {
    /// The list of coin symbols
    pub symbols: Option<String>,
}

/// Response for `pools` endpoint
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolsResponse {
    /// The timestamp of when the response was generated
    pub timestamp: Timestamp,
    /// A map of a coin and its pool depth
    pub pools: HashMap<PoolCoin, Liquidity>,
}

/// Get the current pools
///
/// If `symbols` is empty then all pools will be returned.
/// If `symbols` is not empty then only information for valid symbols will be returned.
///
/// # Example Query
///
/// > GET /v1/pools?symbols=BTC,eth
pub async fn get_pools<S>(
    params: PoolsParams,
    state: Arc<Mutex<S>>,
) -> Result<PoolsResponse, ResponseError>
where
    S: StateProvider,
{
    let pools = state.lock().unwrap().get_pools();

    // Return all pools if no params were passed
    if params.symbols.is_none() {
        return Ok(PoolsResponse {
            timestamp: Timestamp::now(),
            pools,
        });
    }

    // Filter out invalid symbols
    let symbols = params.symbols.unwrap_or("".into());
    let valid_symbols: Vec<Coin> = symbols
        .split(",")
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .filter(|symbol| symbol.clone() != Coin::BASE_COIN)
        .collect();

    let filtered_pools = pools
        .into_iter()
        .filter(|(coin, _)| valid_symbols.contains(&coin.get_coin()))
        .collect();

    return Ok(PoolsResponse {
        timestamp: Timestamp::now(),
        pools: filtered_pools,
    });
}

#[cfg(test)]
mod test {
    use crate::{
        quoter::{api::v1::test::setup_memory_db, BlockProcessor},
        side_chain::{SideChainBlock, LocalEvent},
        utils::test_utils::data::TestData,
    };

    use super::*;

    #[tokio::test]
    async fn returns_correct_response_when_no_symbols_specified() {
        let mut db = setup_memory_db();
        let transactions: Vec<LocalEvent> = vec![
            TestData::pool_change(Coin::BTC, 100, 100).into(),
            TestData::pool_change(Coin::ETH, 75, 75).into(),
            TestData::pool_change(Coin::BTC, -50, 100).into(),
            TestData::pool_change(Coin::BTC, -50, 0).into(),
        ];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        // No symbols

        let result = get_pools(PoolsParams { symbols: None }, db).await;
        assert!(result.is_ok());

        let pools = result.unwrap().pools;

        let btc_pool = pools.get(&PoolCoin::BTC).unwrap();
        assert_eq!(btc_pool.depth, 0);
        assert_eq!(btc_pool.base_depth, 200);

        let eth_pool = pools.get(&PoolCoin::ETH).unwrap();
        assert_eq!(eth_pool.depth, 75);
        assert_eq!(eth_pool.base_depth, 75);
    }

    #[tokio::test]
    async fn returns_correct_response_when_symbols_specified() {
        let mut db = setup_memory_db();
        let transactions: Vec<LocalEvent> = vec![
            TestData::pool_change(Coin::BTC, 100, 100).into(),
            TestData::pool_change(Coin::ETH, 75, 75).into(),
        ];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        let result = get_pools(
            PoolsParams {
                symbols: Some("BTC,btc,Loki,INVALID,,,123".to_string()),
            },
            db,
        )
        .await;
        assert!(result.is_ok());

        let pools = result.unwrap().pools;

        let btc_pool = pools.get(&PoolCoin::BTC).unwrap();
        assert_eq!(btc_pool.depth, 100);
        assert_eq!(btc_pool.base_depth, 100);

        assert_eq!(pools.contains_key(&PoolCoin::ETH), false)
    }

    #[tokio::test]
    async fn returns_correct_response_when_no_pool() {
        let mut db = setup_memory_db();
        let transactions: Vec<LocalEvent> =
            vec![TestData::pool_change(Coin::BTC, 100, 100).into()];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        let result = get_pools(
            PoolsParams {
                symbols: Some("ETH".to_string()),
            },
            db,
        )
        .await;
        assert!(result.is_ok());

        let pools = result.unwrap().pools;

        assert_eq!(pools.contains_key(&PoolCoin::BTC), false);
        assert_eq!(pools.contains_key(&PoolCoin::ETH), false);
    }
}
