use crate::{
    common::{
        api::{self, using},
        coins::Coin,
    },
    quoter::{vault_node::VaultNodeInterface, StateProvider},
    vault::api::v1::PortionsParams,
};
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};
use warp::Filter;

pub mod post_stake;
pub mod post_swap;
pub mod post_unstake;

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

mod get_portions;
pub use get_portions::get_portions;

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

    let stakes = state.lock().unwrap().get_stake_quotes();
    for quote in stakes {
        cache
            .entry(quote.coin_type.get_coin())
            .or_insert(BTreeSet::new())
            .insert(quote.coin_input_address_id);

        cache
            .entry(Coin::LOKI)
            .or_insert(BTreeSet::new())
            .insert(quote.loki_input_address_id.to_string());
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

    let transactions = warp::path!("transactions")
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

    let quote = warp::path!("portions")
        .and(warp::get())
        .and(warp::query::<PortionsParams>())
        .and(using(vault_node.clone()))
        .map(get_portions)
        .and_then(api::respond);

    let swap = warp::path!("swap")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(vault_node.clone()))
        .and(using(input_id_cache.clone()))
        .map(post_swap::swap)
        .and_then(api::respond);

    let stake = warp::path!("stake")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(vault_node.clone()))
        .and(using(input_id_cache.clone()))
        .map(post_stake::stake)
        .and_then(api::respond);

    let unstake = warp::path!("unstake")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(vault_node.clone()))
        .map(post_unstake::unstake)
        .and_then(api::respond);

    warp::path!("v1" / ..) // Add path prefix /v1 to all our routes
        .and(
            coins
                .or(estimate)
                .or(pools)
                .or(transactions)
                .or(quote)
                .or(swap)
                .or(stake)
                .or(unstake),
        )
}
