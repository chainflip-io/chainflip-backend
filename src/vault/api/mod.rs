use super::transactions::TransactionProvider;
use crate::common::api::handle_rejection;
use crate::side_chain::ISideChain;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use warp::Filter;

pub mod v1;

/// Unused
pub struct APIServer {}

impl APIServer {
    /// Starts an http server in the current thread and blocks. Gracefully shutdowns
    /// when `shotdown_receiver` receives a signal (i.e. `send()` is called).
    pub fn serve<S, T>(
        side_chain: Arc<Mutex<S>>,
        provider: Arc<Mutex<T>>,
        shutdown_receiver: oneshot::Receiver<()>,
    ) where
        S: ISideChain + Send + 'static,
        T: TransactionProvider + Send + 'static,
    {
        let routes = v1::endpoints(side_chain, provider).recover(handle_rejection);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let future = async {
            let (_addr, server) =
                warp::serve(routes).bind_with_graceful_shutdown(([127, 0, 0, 1], 3030), async {
                    shutdown_receiver.await.ok();
                });

            server.await;
        };

        rt.block_on(future);
    }
}
