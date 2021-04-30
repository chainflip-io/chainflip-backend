use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use cf_p2p::Communication;
use std::sync::{Arc, Mutex};
use bs58;
use sc_network::{PeerId};

#[rpc]
pub trait RpcApi {
    #[rpc(name = "p2p_send")]
    fn send(&self, peer_id: Option<String>, message: Option<String>) -> Result<u64>;
}

pub struct Rpc<C: Communication> {
    communications: Arc<Mutex<C>>,
}

impl<C: Communication> Rpc<C> {
    pub fn new(communications: Arc<Mutex<C>>) -> Self {
        Rpc {
            communications,
        }
    }
}

impl<C: Communication + Sync + Send + 'static> RpcApi for Rpc<C> {
    fn send(&self, peer_id: Option<String>, message: Option<String>) -> Result<u64> {
        if let Some(peer_id) = peer_id {
            if let Ok(peer_id) = bs58::decode(peer_id.as_bytes()).into_vec() {
                if let Ok(peer_id) = PeerId::from_bytes(&*peer_id) {
                    if let Some(message) = message {
                        self.communications.lock().unwrap().send_message(peer_id, message.into_bytes());
                        return Ok(200);
                    }
                }
            }
        }

        Ok(400)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
