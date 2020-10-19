use crate::{
    common::{
        api::{self, using},
        coins::Coin,
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};
use warp::Filter;

mod post_quote;

mod get_coins;
pub use get_coins::{get_coins, CoinsParams};

mod get_estimate;
pub use get_estimate::{get_estimate, EstimateParams};

mod get_pools;
pub use get_pools::{get_pools, PoolsParams};

#[cfg(test)]
mod test;

/// The v1 API endpoints
pub fn endpoints<V, S>(
    vault_node: Arc<V>,
    state: Arc<Mutex<S>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
where
    V: VaultNodeInterface + Send + Sync,
    S: StateProvider + Send,
{
    let input_id_change: HashMap<Coin, BTreeSet<String>> = HashMap::new();
    let input_id_cache = Arc::new(Mutex::new(input_id_change));

    let coins = warp::path!("coins")
        .and(warp::get())
        .and(warp::query::<CoinsParams>())
        .map(get_coins)
        .and_then(api::respond);

    let estimate = warp::path!("estimate")
        .and(warp::get())
        .and(warp::query::<EstimateParams>())
        .and(using(state.clone()))
        .map(get_estimate)
        .and_then(api::respond);

    let pools = warp::path!("pools")
        .and(warp::get())
        .and(warp::query::<PoolsParams>())
        .and(using(state.clone()))
        .map(get_pools)
        .and_then(api::respond);

    let post_quote_api = warp::path!("quote")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(state.clone()))
        .and(using(vault_node.clone()))
        .and(using(input_id_cache.clone()))
        .map(post_quote::quote)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(coins.or(estimate).or(pools).or(post_quote_api))
}

/// Response for `quote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {}
