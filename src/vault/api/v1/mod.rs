use crate::{
    common::api::{self, using},
    local_store::ILocalStore,
    vault::transactions::TransactionProvider,
};
use chainflip_common::types::Network;
use parking_lot::RwLock;
use std::sync::{Arc, Mutex};
use warp::Filter;

/// Utils
#[macro_use]
pub mod utils;

/// Get events endpoint
pub mod get_events;
use get_events::get_events;

/// Post swap quote endpoint
pub mod post_swap;

/// Post deposit quote endpoint
pub mod post_deposit;

/// Post withdraw request endpoint
pub mod post_withdraw;

/// Get witnesses endpoint
mod get_witnesses;
use get_witnesses::{get_local_witnesses, WitnessQueryParams};

/// Get portions endpoint
mod get_portions;
use get_portions::get_portions;
pub use get_portions::PortionsParams;

use self::get_events::EventsParams;

#[derive(Debug, Clone)]
/// A config object for swap and deposit
pub struct Config {
    /// Loki wallet address
    pub loki_wallet_address: String,
    /// BTC master root key
    pub btc_master_root_key: String,
    /// Network type
    pub net_type: Network,
}

/// The v1 API endpoints
pub fn endpoints<L: ILocalStore + Send, T: TransactionProvider + Send + Sync>(
    local_store: Arc<Mutex<L>>,
    provider: Arc<RwLock<T>>,
    config: Config,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let events = warp::path!("events")
        .and(warp::get())
        .and(warp::query::<EventsParams>())
        .and(using(local_store.clone()))
        .map(get_events)
        .and_then(api::respond);

    let witnesses = warp::path!("witnesses")
        .and(warp::get())
        .and(warp::query::<WitnessQueryParams>())
        .and(using(local_store.clone()))
        .map(get_local_witnesses)
        .and_then(api::respond);

    let swap = warp::path!("swap")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_swap::swap)
        .and_then(api::respond);

    let deposit = warp::path!("deposit")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_deposit::deposit)
        .and_then(api::respond);

    let withdraw = warp::path!("withdraw")
        .and(warp::post())
        .and(warp::body::json())
        .and(using(provider.clone()))
        .and(using(config.clone()))
        .map(post_withdraw::post_withdraw)
        .and_then(api::respond);

    let portions = warp::path!("portions")
        .and(warp::get())
        .and(warp::query::<PortionsParams>())
        .and(using(provider.clone()))
        .map(get_portions)
        .and_then(api::respond);

    // Add path prefix /v1 to all our routes
    warp::path!("v1" / ..).and(
        events
            .or(swap)
            .or(deposit)
            .or(withdraw)
            .or(portions)
            .or(witnesses),
    )
}
