use super::vault_node::VaultNodeInterface;
use super::StateProvider;
use std::sync::{Arc, Mutex};

/// An api server for the quoter
pub struct Server<V, S>
where
    V: VaultNodeInterface,
    S: StateProvider,
{
    api: Arc<V>,
    state: Arc<Mutex<S>>,
}

impl<V, S> Server<V, S>
where
    V: VaultNodeInterface,
    S: StateProvider,
{
    /// Create a new API server.
    pub fn new(api: Arc<V>, state: Arc<Mutex<S>>) -> Self {
        Server { api, state }
    }

    pub async fn serve(&self, port: u16) {
        info!("Quoter API Server listening on port {}", port);
    }
}
