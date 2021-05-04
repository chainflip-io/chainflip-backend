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
use futures::{StreamExt, TryStreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use jsonrpc_core::futures::Sink;
use cf_p2p::Observer;

#[rpc]
pub trait RpcApi {
    /// RPC Metadata
    type Metadata;

    #[rpc(name = "p2p_send")]
    fn send(&self, peer_id: Option<String>, message: Option<String>) -> Result<u64>;

    #[rpc(name = "p2p_broadcast")]
    fn broadcast(&self, message: Option<String>) -> Result<u64>;

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

pub struct P2PStream<T> {
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
    pub fn new<E>(executor: Arc<E>) -> (Self, Arc<P2PStream<Vec<u8>>>)
        where E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
    {
        let stream = Arc::new(P2PStream::new());
        (RpcParams {
            stream: stream.clone(),
            manager: SubscriptionManager::new(executor),
        }, stream.clone())
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
            subscriber.unbounded_send(messages.clone());
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
                        self.comms.lock().unwrap().send_message(&peer_id, message.into_bytes());
                        return Ok(200);
                    }
                }
            }
        }

        Ok(400)
    }

    fn broadcast(&self, message: Option<String>) -> Result<u64> {
        if let Some(message) = message {
            self.comms.lock().unwrap().broadcast(message.into_bytes());
            return Ok(200);
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
    use super::*;
    use sc_rpc::testing::TaskExecutor;
    use jsonrpc_core::{Notification, Output, types::Params};
    use sp_core::Decode;
    use std::collections::HashMap;

    struct P2P {
        nodes: HashMap<PeerId, Node>,
    }
    impl P2P {
        fn new() -> Self {
            P2P {
                nodes: HashMap::new()
            }
        }

        fn get_node(&mut self, peer_id: &PeerId) -> Option<&Node> {
            self.nodes.get(peer_id)
        }

        fn create_node(&mut self) -> PeerId {
            let node = Node::new();
            let peer_id = node.peer_id;
            self.nodes.insert(peer_id, node);
            peer_id
        }

        fn broadcast(&mut self, data: Message) {
            for (_, node) in &self.nodes {
                node.communications.lock().unwrap().broadcast(data.clone());
            }
        }

        fn send_message(&mut self, peer_id: &PeerId, data: Message) {
            if let Some(node) = self.nodes.get(&peer_id) {
                node.communications.lock().unwrap().send_message(peer_id, data.clone());
            }
        }
    }

    struct Node {
        peer_id: PeerId,
        io: jsonrpc_core::MetaIoHandler<sc_rpc::Metadata>,
        communications: Arc<Mutex<Communications>>,
    }

    struct Communications {
        stream: Arc<P2PStream<Vec<u8>>>,
    }

    impl Communication for Communications {
        fn send_message(&mut self, peer_id: &PeerId, data: Message) {
            let subscribers = self.stream.subscribers.lock().unwrap();
            for mut subscriber in subscribers.iter() {
                subscriber.unbounded_send(data.clone());
            }
        }

        fn broadcast(&self, data: Message) {
            let subscribers = self.stream.subscribers.lock().unwrap();
            for mut subscriber in subscribers.iter() {
                subscriber.unbounded_send(data.clone());
            }
        }
    }

    impl Node {
        fn new() -> Self {
            let executor = Arc::new(TaskExecutor);
            let (rpc_params, stream) = RpcParams::new(executor);
            let mut io = jsonrpc_core::MetaIoHandler::default();
            let communications = Communications { stream };
            let communications = Arc::new(Mutex::new(communications));
            let rpc = Rpc::new(communications.clone(), Arc::new(rpc_params));
            io.extend_with(RpcApi::to_delegate(rpc));

            Node { peer_id: PeerId::random(), io, communications }
        }
    }

    fn setup_session() -> (sc_rpc::Metadata, jsonrpc_core::futures::sync::mpsc::Receiver<String>) {
        let (tx, rx) = jsonrpc_core::futures::sync::mpsc::channel(2);
        let meta = sc_rpc::Metadata::new(tx);
        (meta, rx)
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let node= Node::new();

        let (meta, _) = setup_session();

        let sub_request = r#"{"jsonrpc":"2.0","method":"cf_p2p_subscribeNotifications","params":[],"id":1}"#;
        let resp = node.io.handle_request_sync(sub_request, meta.clone());
        let resp: Output = serde_json::from_str(&resp.unwrap()).unwrap();

        let sub_id = match resp {
            Output::Success(success) => success.result,
            _ => panic!(),
        };

        let unsub_req = format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"cf_p2p_unsubscribeNotifications\",\"params\":[{}],\"id\":1}}",
            sub_id
        );
        assert_eq!(
            node.io.handle_request_sync(&unsub_req, meta.clone()),
            Some(r#"{"jsonrpc":"2.0","result":true,"id":1}"#.into()),
        );

        assert_eq!(
            node.io.handle_request_sync(&unsub_req, meta),
            Some(r#"{"jsonrpc":"2.0","result":false,"id":1}"#.into()),
        );
    }

    #[test]
    fn send_message() {
        let node= Node::new();

        let peer = PeerId::random();
        let request = format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"p2p_send\",\"params\":[\"{}\", \"{}\"],\"id\":1}}",
            peer.to_base58(), "hello",
        );
        let meta = sc_rpc::Metadata::default();
        assert_eq!(node.io.handle_request_sync(&request, meta), Some("{\"jsonrpc\":\"2.0\",\"result\":200,\"id\":1}".to_string()));
    }

    #[test]
    fn broadcast_message() {
        let node= Node::new();

        let peer = PeerId::random();
        let request = format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"p2p_broadcast\",\"params\":[\"{}\"],\"id\":1}}",
            "hello",
        );
        let meta = sc_rpc::Metadata::default();
        assert_eq!(node.io.handle_request_sync(&request, meta), Some("{\"jsonrpc\":\"2.0\",\"result\":200,\"id\":1}".to_string()));
    }

    #[test]
    fn subscribe_and_listen_for_messages() {
        let mut p2p = P2P::new();
        let peer_id = p2p.create_node();
        let node = p2p.get_node(&peer_id).unwrap();
        let (meta, receiver) = setup_session();

        let sub_request = r#"{"jsonrpc":"2.0","method":"cf_p2p_subscribeNotifications","params":[],"id":1}"#;
        let resp = node.io.handle_request_sync(sub_request, meta.clone());
        let mut resp: serde_json::Value = serde_json::from_str(&resp.unwrap()).unwrap();
        let sub_id: String = serde_json::from_value(resp["result"].take()).unwrap();

        // Simulate a message being received from the peer
        let message: Vec<u8> = vec![1,2,3];
        p2p.send_message(&peer_id, message.clone());

        // We should get a notification of this event
        let recv = receiver.take(1).wait().flatten().collect::<Vec<_>>();
        let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
        let mut json_map = match recv.params {
            Params::Map(json_map) => json_map,
            _ => panic!(),
        };

        let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
        let recv_message: Vec<u8> = serde_json::from_value(json_map["result"].take()).unwrap();

        assert_eq!(recv.method, "cf_p2p_notifications");
        assert_eq!(recv_sub_id, sub_id);
        assert_eq!(recv_message, message);
    }

    #[test]
    fn subscribe_and_listen_for_broadcast() {
        // Create a node and subscribe to it
        let mut p2p = P2P::new();
        let peer_id = p2p.create_node();
        let node = p2p.get_node(&peer_id).unwrap();

        let (meta, receiver) = setup_session();
        let sub_request = r#"{"jsonrpc":"2.0","method":"cf_p2p_subscribeNotifications","params":[],"id":1}"#;
        let resp = node.io.handle_request_sync(sub_request, meta.clone());
        let mut resp: serde_json::Value = serde_json::from_str(&resp.unwrap()).unwrap();
        let sub_id: String = serde_json::from_value(resp["result"].take()).unwrap();

        // Create another node and subscribe to it
        let peer_id_1 = p2p.create_node();
        let node_1 = p2p.get_node(&peer_id).unwrap();
        let (meta_1, receiver_1) = setup_session();
        let sub_request_1 = r#"{"jsonrpc":"2.0","method":"cf_p2p_subscribeNotifications","params":[],"id":1}"#;
        let resp_1 = node_1.io.handle_request_sync(sub_request_1, meta_1.clone());
        let mut resp_1: serde_json::Value = serde_json::from_str(&resp_1.unwrap()).unwrap();
        let sub_id_1: String = serde_json::from_value(resp_1["result"].take()).unwrap();

        // Simulate a message being received from the peer
        let message: Vec<u8> = vec![1,2,3];
        p2p.broadcast(message.clone());

        // We should get a notification of this event
        let recv = receiver.take(1).wait().flatten().collect::<Vec<_>>();
        let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
        let mut json_map = match recv.params {
            Params::Map(json_map) => json_map,
            _ => panic!(),
        };

        let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
        let recv_message: Vec<u8> = serde_json::from_value(json_map["result"].take()).unwrap();

        assert_eq!(recv.method, "cf_p2p_notifications");
        assert_eq!(recv_sub_id, sub_id);
        assert_eq!(recv_message, message.clone());

        let recv = receiver_1.take(1).wait().flatten().collect::<Vec<_>>();
        let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
        let mut json_map = match recv.params {
            Params::Map(json_map) => json_map,
            _ => panic!(),
        };

        let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
        let recv_message: Vec<u8> = serde_json::from_value(json_map["result"].take()).unwrap();

        assert_eq!(recv.method, "cf_p2p_notifications");
        assert_eq!(recv_sub_id, sub_id_1);
        assert_eq!(recv_message, message);
    }
}
