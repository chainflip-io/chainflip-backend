use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use cf_p2p::Communication;
use std::sync::{Arc, Mutex};
use bs58;
use sc_network::{PeerId};
use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use std::marker::Send;
use log::warn;
use futures::{StreamExt, TryStreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use jsonrpc_core::futures::Sink;

#[rpc]
pub trait RpcApi {
    /// RPC Metadata
    type Metadata;

    #[rpc(name = "p2p_send")]
    fn send(&self, peer_id: Option<String>, message: Option<String>) -> Result<u64>;

    /// Subscribe to receive notifications
    #[pubsub(
        subscription = "cf_p2p_notifications",
        subscribe,
        name = "cf_p2p_subscribeNotifications"
    )]
    fn subscribe_notifications(
        &self,
        metadata: Self::Metadata,
        subscriber: Subscriber<Vec<u8>>
    );

    /// Unsubscribe from receiving notifications
    #[pubsub(
        subscription = "cf_p2p_notifications",
        unsubscribe,
        name = "cf_p2p_unsubscribeNotifications"
    )]
    fn unsubscribe_notifications(
        &self,
        metadata: Option<Self::Metadata>,
        id: SubscriptionId
    ) -> jsonrpc_core::Result<bool>;
}

struct P2PStream<T> {
    subscribers: Arc<Mutex<Vec<UnboundedSender<T>>>>,
}

impl<T> P2PStream<T> {
    fn new() -> Self {
        let subscribers = Arc::new(Mutex::new(vec![]));
        P2PStream {
            subscribers,
        }
    }

    fn subscribe(&self) -> UnboundedReceiver<T> {
        let (tx, rx) = unbounded();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }
}

pub struct Rpc<C: Communication> {
    communications: Arc<Mutex<C>>,
    manager: SubscriptionManager,
    stream: P2PStream<Vec<u8>>,
}

impl<C: Communication> Rpc<C> {
    pub fn new<E>(communications: Arc<Mutex<C>>, executor: Arc<E>) -> Self
        where E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
    {
        let manager = SubscriptionManager::new(executor);
        let stream = P2PStream::new();

        Rpc {
            communications,
            manager,
            stream,
        }
    }
}

impl<C: Communication + Sync + Send + 'static> RpcApi
    for Rpc<C>
{
    type Metadata = sc_rpc::Metadata;

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

    fn subscribe_notifications(
        &self,
        _metadata: Self::Metadata,
        subscriber: Subscriber<Vec<u8>>
    ) {
        let stream = self.stream.subscribe()
            .map(|x| Ok::<_,()>(x))
            .map_err(|e| warn!("Notification stream error: {:?}", e))
            .compat();

        self.manager.add(subscriber, |sink| {
            let stream = stream.map(|res| Ok(res));
            sink.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
                .send_all(stream)
                .map(|_| ())
        });
    }

    fn unsubscribe_notifications(
        &self,
        _metadata: Option<Self::Metadata>,
        id: SubscriptionId
    ) -> jsonrpc_core::Result<bool> {
        Ok(self.manager.cancel(id))
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
