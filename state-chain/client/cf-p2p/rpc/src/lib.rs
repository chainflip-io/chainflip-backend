use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use cf_p2p::Communication;
use std::sync::Arc;

#[rpc]
pub trait RpcApi {
    #[rpc(name = "p2p_send")]
    fn send(&self, peer_id: Option<String>) -> Result<u64>;
}

pub struct Rpc<C: Communication> {
    communications: Arc<C>,
}

impl<C: Communication> Rpc<C> {
    pub fn new(communications: Arc<C>) -> Self {
        Rpc {
            communications,
        }
    }
}
impl<C: Communication + Sync + Send + 'static> RpcApi for Rpc<C> {
    fn send(&self, peer_id: Option<String>) -> Result<u64> {
        Ok(200)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
