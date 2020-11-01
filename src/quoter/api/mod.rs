use super::vault_node::VaultNodeInterface;
use super::StateProvider;
use crate::common::api;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use warp::Filter;

mod v1;

/// An API server for the quoter
pub struct API {}

impl API {
    /// Starts an http server in the current thread and blocks
    pub fn serve<V, S>(addr: impl Into<SocketAddr>, vault_node: Arc<V>, state: Arc<Mutex<S>>)
    where
        V: VaultNodeInterface + Send + Sync + 'static,
        S: StateProvider + Send + 'static,
    {
        // Temporary open to all origins for testing
        let cors = warp::cors()
            .allow_any_origin()
            .allow_methods(vec!["GET", "POST", "DELETE", "HEAD", "OPTIONS", "PUT"]);
        let routes = v1::endpoints(vault_node, state)
            .with(cors)
            .recover(api::handle_rejection);

        let future = async { warp::serve(routes).run(addr).await };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(future);
    }
}
