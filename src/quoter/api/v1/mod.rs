use crate::{
    common::{
        api::{self, using},
        coins::Coin,
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};
use warp::Filter;

mod post_swap;

mod get_coins;
pub use get_coins::{get_coins, CoinsParams};

mod get_estimate;
pub use get_estimate::{get_estimate, EstimateParams};

mod get_pools;
pub use get_pools::{get_pools, PoolsParams};

mod get_quote;
pub use get_quote::{get_quote, GetQuoteParams};

mod get_transactions;
pub use get_transactions::{get_transactions, TransactionsParams};

// Util functions
pub mod utils;

#[cfg(test)]
mod test;

/// Get a pre-populated input id cache
pub fn get_input_id_cache<S>(state: &Arc<Mutex<S>>) -> HashMap<Coin, BTreeSet<String>>
where
    S: StateProvider,
{
    let mut cache: HashMap<Coin, BTreeSet<String>> = HashMap::new();
    let quotes = state.lock().unwrap().get_swap_quotes();

    for quote in quotes {
        cache
            .entry(quote.input)
            .or_insert(BTreeSet::new())
            .insert(quote.input_address_id);
    }

    cache
}

/// The v1 API endpoints
pub fn endpoints<V, S>(
    vault_node: Arc<V>,
    state: Arc<Mutex<S>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone
where
    V: VaultNodeInterface + Send + Sync,
    S: StateProvider + Send,
{
    // Pre populate cache
    let input_id_cache = get_input_id_cache(&state);
    let input_id_cache = Arc::new(Mutex::new(input_id_cache));

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

    let transactions = warp::path!("quote")
        .and(warp::get())
        .and(warp::query::<TransactionsParams>())
        .and(using(state.clone()))
        .map(get_transactions)
        .and_then(api::respond);

    let quote = warp::path!("quote")
        .and(warp::get())
        .and(warp::query::<GetQuoteParams>())
        .and(using(state.clone()))
        .map(get_quote)
        .and_then(api::respond);

    let post_swap_api = warp::path!("swap")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(vault_node.clone()))
        .and(using(input_id_cache.clone()))
        .map(post_swap::swap)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(
            coins
                .or(estimate)
                .or(pools)
                .or(transactions)
                .or(quote)
                .or(post_swap_api),
        )
}
