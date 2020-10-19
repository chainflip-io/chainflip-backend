use crate::{
    common::{api::ResponseError, coins::Coin, Liquidity, PoolCoin, Timestamp},
    quoter::StateProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Parameters for `pools` endpoint
#[derive(Debug, Deserialize)]
pub struct PoolsParams {
    /// The list of coin symbols
    pub symbols: Option<Vec<String>>,
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
    let valid_symbols: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .filter(|symbol| symbol.clone() != Coin::LOKI)
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
        quoter::database::Database, quoter::BlockProcessor, side_chain::SideChainBlock,
        side_chain::SideChainTx, transactions::PoolChangeTx,
    };
    use rusqlite::Connection;

    use super::*;

    fn setup() -> Database {
        let connection = Connection::open_in_memory().expect("Failed to open connection");
        Database::new(connection)
    }

    #[tokio::test]
    async fn returns_correct_response_when_no_symbols_specified() {
        let mut db = setup();
        let transactions: Vec<SideChainTx> = vec![
            PoolChangeTx::new(PoolCoin::BTC, 100, 100).into(),
            PoolChangeTx::new(PoolCoin::ETH, 75, 75).into(),
            PoolChangeTx::new(PoolCoin::BTC, 100, -50).into(),
            PoolChangeTx::new(PoolCoin::BTC, 0, -50).into(),
        ];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            txs: transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        // No symbols

        let result = get_pools(PoolsParams { symbols: None }, db).await;
        assert!(result.is_ok());

        let pools = result.unwrap().pools;

        let btc_pool = pools.get(&PoolCoin::BTC).unwrap();
        assert_eq!(btc_pool.depth, 0);
        assert_eq!(btc_pool.loki_depth, 200);

        let eth_pool = pools.get(&PoolCoin::ETH).unwrap();
        assert_eq!(eth_pool.depth, 75);
        assert_eq!(eth_pool.loki_depth, 75);
    }

    #[tokio::test]
    async fn returns_correct_response_when_symbols_specified() {
        let mut db = setup();
        let transactions: Vec<SideChainTx> = vec![
            PoolChangeTx::new(PoolCoin::BTC, 100, 100).into(),
            PoolChangeTx::new(PoolCoin::ETH, 75, 75).into(),
        ];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            txs: transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        let result = get_pools(
            PoolsParams {
                symbols: Some(vec![
                    "BTC".to_string(),
                    "btc".to_string(),
                    "Loki".to_string(),
                ]),
            },
            db,
        )
        .await;
        assert!(result.is_ok());

        let pools = result.unwrap().pools;

        let btc_pool = pools.get(&PoolCoin::BTC).unwrap();
        assert_eq!(btc_pool.depth, 100);
        assert_eq!(btc_pool.loki_depth, 100);

        assert_eq!(pools.contains_key(&PoolCoin::ETH), false)
    }

    #[tokio::test]
    async fn returns_correct_response_when_no_pool() {
        let mut db = setup();
        let transactions: Vec<SideChainTx> =
            vec![PoolChangeTx::new(PoolCoin::BTC, 100, 100).into()];

        db.process_blocks(&[SideChainBlock {
            id: 0,
            txs: transactions,
        }])
        .unwrap();

        let db = Arc::new(Mutex::new(db));

        let result = get_pools(
            PoolsParams {
                symbols: Some(vec!["ETH".to_string()]),
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
