use super::vault_node::VaultNodeInterface;
use super::StateProvider;
use crate::common::coins::{Coin, CoinInfo};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use warp::Filter;

/// Parameters for `/v1/coins` endpoint
#[derive(Debug, Deserialize)]
struct CoinsParams {
    symbols: Option<Vec<String>>,
}

/// An api server for the quoter
pub struct API {}

impl API {
    /// Starts an http server in the current thread and blocks
    pub fn serve<V, S>(port: u16, vault_node: Arc<V>, state: Arc<Mutex<S>>)
    where
        V: VaultNodeInterface,
        S: StateProvider,
    {
        let _vault_node_ref = warp::any().map(move || vault_node.clone());
        let _state_ref = warp::any().map(move || state.clone());

        let coins = warp::get()
            .and(warp::path!("v1" / "coins"))
            .and(warp::query::<CoinsParams>())
            .map(|params| API::get_coins(params))
            .map(|res| serde_json::to_string(&res).unwrap());

        let future = async { warp::serve(coins).run(([127, 0, 0, 1], port)).await };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(future);
    }

    /// Get coins that we support.
    ///
    /// If `symbols` is empty then all coins will be returned.
    /// If `symbols` is not empty then only information for valid symbols will be returned.
    ///
    /// # Example Query
    ///
    /// > GET /v1/coins?symbols=BTC,loki
    fn get_coins(params: CoinsParams) -> Vec<CoinInfo> {
        // Return all coins if no params were passed
        if params.symbols.is_none() {
            return Coin::ALL.iter().map(|coin| coin.get_info()).collect();
        }

        // Filter out invalid coins
        let valid_coins: Vec<Coin> = params
            .symbols
            .unwrap()
            .iter()
            .filter_map(|symbol| symbol.parse::<Coin>().ok())
            .collect();

        let mut info: Vec<CoinInfo> = vec![];

        for coin in valid_coins {
            info.push(coin.get_info());
        }

        return info;
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    pub fn get_coins_returns_all_coins() {
        let params = CoinsParams { symbols: None };
        let result = API::get_coins(params);
        assert_eq!(result.len(), Coin::ALL.len());
    }

    #[test]
    pub fn get_coins_returns_coin_information() {
        let params = CoinsParams {
            symbols: Some(vec![
                "eth".to_owned(),
                "LOKI".to_owned(),
                "invalid_coin".to_owned(),
            ]),
        };
        let result = API::get_coins(params);

        assert_eq!(result.len(), 2, "Expected get_coins to return 2 CoinInfo");

        for info in result {
            match info.symbol {
                Coin::ETH | Coin::LOKI => continue,
                coin @ _ => panic!("Result returned unexpected coin: {}", coin),
            }
        }
    }
}
