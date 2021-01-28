use super::{config::VaultConfig, transactions::TransactionProvider};
use crate::common::api::handle_rejection;
use crate::side_chain::ISideChain;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use parking_lot::RwLock;
use tokio::sync::oneshot;
use warp::Filter;

/// Api v1
pub mod v1;

/// Unused
pub struct APIServer {}

impl APIServer {
    /// Starts an http server in the current thread and blocks. Gracefully shutdowns
    /// when `shotdown_receiver` receives a signal (i.e. `send()` is called).
    pub fn serve<S, T>(
        config: &VaultConfig,
        side_chain: Arc<Mutex<S>>,
        provider: Arc<RwLock<T>>,
        shutdown_receiver: oneshot::Receiver<()>,
    ) where
        S: ISideChain + Send + 'static,
        T: TransactionProvider + Send + Sync + 'static,
    {
        let config = v1::Config {
            loki_wallet_address: config.loki.wallet_address.clone(),
            btc_master_root_key: config.btc.master_root_key.clone(),
            net_type: config.net_type,
        };
        let routes = v1::endpoints(side_chain, provider, config).recover(handle_rejection);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let addr: SocketAddr = (([127, 0, 0, 1], 3030)).into();

        info!("Vault rpc is initialized at: {}", addr);

        let future = async {
            let (_addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_receiver.await.ok();
            });

            server.await;
        };

        rt.block_on(future);
    }
}
