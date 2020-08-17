use super::vault_node::VaultNodeInterface;
use super::StateProvider;
use crate::common::api;
use std::sync::{Arc, Mutex};
use warp::Filter;

mod v1;

/// An API server for the quoter
pub struct API {}

impl API {
    /// Starts an http server in the current thread and blocks
    pub fn serve<V, S>(port: u16, vault_node: Arc<V>, state: Arc<Mutex<S>>)
    where
        V: VaultNodeInterface + 'static,
        S: StateProvider + 'static,
    {
        let routes = v1::endpoints(vault_node, state).recover(api::handle_rejection);

        let future = async { warp::serve(routes).run(([127, 0, 0, 1], port)).await };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(future);
    }
}
