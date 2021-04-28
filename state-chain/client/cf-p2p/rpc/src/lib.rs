use jsonrpc_core::Result;
use jsonrpc_derive::rpc;

#[rpc]
pub trait RpcCalls {
    #[rpc(name = "p2p_send")]
    fn send(&self, peer_id: Option<String>) -> Result<u64>;
}

pub struct Rpc {}

impl Rpc {
    pub fn new() -> Self {
        Rpc {
        }
    }
}
impl RpcCalls for Rpc {
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
