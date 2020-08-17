use crate::{
    common::{
        api,
        api::ResponseError,
        coins::{Coin, CoinInfo},
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use warp::Filter;

/// Parameters for `coins` endpoint
#[derive(Debug, Deserialize)]
pub struct CoinsParams {
    symbols: Option<Vec<String>>,
}

/// The v1 API endpoints
pub fn endpoints<V, S>(
    vault_node: Arc<V>,
    state: Arc<Mutex<S>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
where
    V: VaultNodeInterface,
    S: StateProvider,
{
    let _vault_node_ref = warp::any().map(move || vault_node.clone());
    let _state_ref = warp::any().map(move || state.clone());

    let coins = warp::path!("coins")
        .and(warp::get())
        .and(warp::query::<CoinsParams>())
        .map(get_coins)
        .and_then(api::respond);

    warp::path!("api" / "v1" / ..) // Add path prefix /api/v1 to all our routes
        .and(coins) // .and(coins.or(another).or(yet_another))
}

/// Get coins that we support.
///
/// If `symbols` is empty then all coins will be returned.
/// If `symbols` is not empty then only information for valid symbols will be returned.
///
/// # Example Query
///
/// > GET /v1/coins?symbols=BTC,loki
pub fn get_coins(params: CoinsParams) -> Result<Vec<CoinInfo>, ResponseError> {
    // Return all coins if no params were passed
    if params.symbols.is_none() {
        return Ok(Coin::ALL.iter().map(|coin| coin.get_info()).collect());
    }

    // Filter out invalid coins
    let valid_coins: Vec<Coin> = params
        .symbols
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol.parse::<Coin>().ok())
        .collect();

    let info = valid_coins.iter().map(|coin| coin.get_info()).collect();

    return Ok(info);
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    pub fn get_coins_returns_all_coins() {
        let params = CoinsParams { symbols: None };
        let result = get_coins(params).expect("Expected result to be Ok.");
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
        let result = get_coins(params).expect("Expected result to be Ok.");

        assert_eq!(result.len(), 2, "Expected get_coins to return 2 CoinInfo");

        for info in result {
            match info.symbol {
                Coin::ETH | Coin::LOKI => continue,
                coin @ _ => panic!("Result returned unexpected coin: {}", coin),
            }
        }
    }
}
