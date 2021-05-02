use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use cf_p2p::{Communication, Message};
use std::sync::{Arc, Mutex};
use bs58;
use sc_network::{PeerId};
use jsonrpc_pubsub::{typed::Subscriber, SubscriptionId, manager::SubscriptionManager};
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use std::marker::Send;
use log::warn;
use futures::{StreamExt, TryStreamExt, SinkExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use jsonrpc_core::futures::Sink;
use cf_p2p::Observer;

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

pub struct RpcParams {
    stream: Arc<P2PStream<Vec<u8>>>,
    manager: SubscriptionManager,
}

impl RpcParams {
    pub fn new<E>(executor: Arc<E>) -> Self
        where E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
    {
        RpcParams {
            stream: Arc::new(P2PStream::new()),
            manager: SubscriptionManager::new(executor),
        }
    }
}

impl Observer for RpcParams {
    fn new_peer(&self, peer_id: &PeerId) {
        // self.stream.
    }

    fn disconnected(&self, peer_id: &PeerId) {
        //self.stream
    }

    // Notify subscribers of message received, yes we are not filtering yet
    fn received(&self, peer_id: &PeerId, messages: Message) {
        let subscribers = self.stream.subscribers.lock().unwrap();
        for mut subscriber in subscribers.iter() {
            subscriber.send(messages.clone());
        }
    }
}

pub struct Rpc<C: Communication> {
    params: Arc<RpcParams>,
    comms: Arc<Mutex<C>>,
}

impl<C: Communication> Rpc<C> {
    pub fn new(comms: Arc<Mutex<C>>, params: Arc<RpcParams>) -> Self {
        Rpc {
            comms,
            params,
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
                        self.comms.lock().unwrap().send_message(peer_id, message.into_bytes());
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
        let stream = self.params.stream.subscribe()
            .map(|x| Ok::<_,()>(x))
            .map_err(|e| warn!("Notification stream error: {:?}", e))
            .compat();

        self.params.manager.add(subscriber, |sink| {
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
        Ok(self.params.manager.cancel(id))
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
