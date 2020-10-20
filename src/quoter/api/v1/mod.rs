use crate::{
    common::{
        api::{self, using},
        coins::Coin,
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
};
use rand::{prelude::StdRng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::SystemTime,
};
use warp::Filter;

mod post_quote;

mod get_coins;
pub use get_coins::{get_coins, CoinsParams};

mod get_estimate;
pub use get_estimate::{get_estimate, EstimateParams};

mod get_pools;
pub use get_pools::{get_pools, PoolsParams};

mod get_quote;
pub use get_quote::{get_quote, GetQuoteParams};

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

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Duration since UNIX_EPOCH failed");
    let rng = StdRng::seed_from_u64(now.as_secs());

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

    let get_quote_api = warp::path!("quote")
        .and(warp::get())
        .and(warp::query::<GetQuoteParams>())
        .and(using(state.clone()))
        .map(get_quote)
        .and_then(api::respond);

    let post_quote_api = warp::path!("quote")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(state.clone()))
        .and(using(vault_node.clone()))
        .and(using(input_id_cache.clone()))
        .and(using(rng))
        .map(post_quote::quote)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(coins.or(estimate).or(pools).or(get_quote_api).or(post_quote_api))
}