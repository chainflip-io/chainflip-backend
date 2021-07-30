use cf_p2p::{Message, Messaging, NetworkObserver};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{StreamExt, TryStreamExt};
use jsonrpc_core::futures::Sink;
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use jsonrpc_core::Error;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{manager::SubscriptionManager, typed::Subscriber, SubscriptionId};
use log::{debug, warn};
use sc_network::config::identity::ed25519;
use sc_network::config::PublicKey;
use sc_network::PeerId;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sp_core::ed25519::Public;
use std::marker::Send;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

type ValidatorId = String;

#[rpc]
pub trait RpcApi {
	/// RPC Metadata
	type Metadata;

	/// Send a message to validator id returning a HTTP status code
	#[rpc(name = "p2p_send")]
	fn send(&self, validator_id: ValidatorId, message: String) -> Result<u64>;

	/// Broadcast a message to the p2p network returning a HTTP status code
	#[rpc(name = "p2p_broadcast")]
	fn broadcast(&self, message: String) -> Result<u64>;

	/// Subscribe to receive notifications
	#[pubsub(
		subscription = "cf_p2p_notifications",
		subscribe,
		name = "cf_p2p_subscribeNotifications"
	)]
	fn subscribe_notifications(&self, metadata: Self::Metadata, subscriber: Subscriber<P2PEvent>);

	/// Unsubscribe from receiving notifications
	#[pubsub(
		subscription = "cf_p2p_notifications",
		unsubscribe,
		name = "cf_p2p_unsubscribeNotifications"
	)]
	fn unsubscribe_notifications(
		&self,
		metadata: Option<Self::Metadata>,
		id: SubscriptionId,
	) -> jsonrpc_core::Result<bool>;
}

/// A list of subscribers to the p2p message events coming in from cf-p2p
pub struct P2PStream<T> {
	subscribers: Arc<Mutex<Vec<UnboundedSender<T>>>>,
}

impl<T> P2PStream<T> {
	fn new() -> Self {
		let subscribers = Arc::new(Mutex::new(vec![]));
		P2PStream { subscribers }
	}

	/// A new subscriber to be notified on upcoming events
	fn subscribe(&self) -> UnboundedReceiver<T> {
		let (tx, rx) = unbounded();
		self.subscribers.lock().unwrap().push(tx);
		rx
	}
}

/// An event stream over type `RpcEvent`
type EventStream = Arc<P2PStream<P2PEvent>>;

/// Our core bridge between p2p events and our RPC subscribers
pub struct RpcCore {
	stream: EventStream,
	manager: SubscriptionManager,
}

type RpcPeerId = String;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum P2PEvent {
	Received(RpcPeerId, Message),
	PeerConnected(RpcPeerId),
	PeerDisconnected(RpcPeerId),
}

impl RpcCore {
	pub fn new<E>(executor: Arc<E>) -> (Self, EventStream)
	where
		E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
	{
		let stream = Arc::new(P2PStream::new());
		(
			RpcCore {
				stream: stream.clone(),
				manager: SubscriptionManager::new(executor),
			},
			stream.clone(),
		)
	}

	/// Notify to our subscribers
	fn notify(&self, event: P2PEvent) {
		let subscribers = self.stream.subscribers.lock().unwrap();
		for subscriber in subscribers.iter() {
			if let Err(e) = subscriber.unbounded_send(event.clone()) {
				debug!("Failed to send message: {:?}", e);
			}
		}
	}
}

/// Observe p2p events and notify subscribers
impl NetworkObserver for RpcCore {
	fn new_peer(&self, peer_id: &PeerId) {
		self.notify(P2PEvent::PeerConnected(peer_id.to_base58()));
	}

	fn disconnected(&self, peer_id: &PeerId) {
		self.notify(P2PEvent::PeerDisconnected(peer_id.to_base58()));
	}

	fn received(&self, peer_id: &PeerId, messages: Message) {
		self.notify(P2PEvent::Received(peer_id.to_base58(), messages.clone()));
	}
}

/// The RPC bridge and API
pub struct Rpc<C: Messaging> {
	core: Arc<RpcCore>,
	messaging: Arc<Mutex<C>>,
}

impl<C: Messaging> Rpc<C> {
	pub fn new(messaging: Arc<Mutex<C>>, core: Arc<RpcCore>) -> Self {
		Rpc { messaging, core }
	}
}

fn peer_id_from_validator_id(validator_id: &ValidatorId) -> std::result::Result<PeerId, &str> {
	Public::from_str(validator_id)
		.map_err(|_| "failed parsing")
		.and_then(|p| ed25519::PublicKey::decode(&p.0).map_err(|_| "failed decoding"))
		.and_then(|p| Ok(PeerId::from_public_key(PublicKey::Ed25519(p))))
}

/// Impl of the `RpcApi` - send, broadcast and subscribe to notifications
impl<C: Messaging + Sync + Send + 'static> RpcApi for Rpc<C> {
	type Metadata = sc_rpc::Metadata;

	fn send(&self, validator_id: ValidatorId, message: String) -> Result<u64> {
		if let Ok(peer_id) = peer_id_from_validator_id(&validator_id) {
			return if self
				.messaging
				.lock()
				.unwrap()
				.send_message(&peer_id, message.into_bytes())
			{
				Ok(200)
			} else {
				Err(Error::invalid_params("invalid message and/or peer"))
			};
		}

		Err(Error::invalid_request())
	}

	fn broadcast(&self, message: String) -> Result<u64> {
		if self
			.messaging
			.lock()
			.unwrap()
			.broadcast(message.into_bytes())
		{
			return Ok(200);
		}

		Err(Error::invalid_request())
	}

	fn subscribe_notifications(&self, _metadata: Self::Metadata, subscriber: Subscriber<P2PEvent>) {
		let stream = self
			.core
			.stream
			.subscribe()
			.map(|x| Ok::<_, ()>(x))
			.map_err(|e| warn!("Notification stream error: {:?}", e))
			.compat();

		self.core.manager.add(subscriber, |sink| {
			let stream = stream.map(|evt| Ok(evt));
			sink.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
				.send_all(stream)
				.map(|_| ())
		});
	}

	fn unsubscribe_notifications(
		&self,
		_metadata: Option<Self::Metadata>,
		id: SubscriptionId,
	) -> jsonrpc_core::Result<bool> {
		Ok(self.core.manager.cancel(id))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpc_core::{types::Params, Notification, Output};
	use sc_network::config::identity::ed25519;
	use sc_network::config::PublicKey;
	use sc_rpc::testing::TaskExecutor;
	use sp_core::ed25519::Public;
	use std::collections::HashMap;

	/// Our network of nodes
	struct P2P {
		nodes: HashMap<PeerId, Node>,
	}

	impl P2P {
		fn new() -> Self {
			P2P {
				nodes: HashMap::new(),
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
				node.messenger.lock().unwrap().broadcast(data.clone());
			}
		}

		fn send_message(&mut self, peer_id: &PeerId, data: Message) {
			if let Some(node) = self.nodes.get(&peer_id) {
				node.messenger
					.lock()
					.unwrap()
					.send_message(peer_id, data.clone());
			}
		}
	}

	/// A node on this test network
	struct Node {
		peer_id: PeerId,
		io: jsonrpc_core::MetaIoHandler<sc_rpc::Metadata>,
		messenger: Arc<Mutex<Messenger>>,
		stream: Arc<EventStream>,
	}

	struct Messenger {
		stream: Arc<EventStream>,
	}

	impl Messaging for Messenger {
		fn send_message(&mut self, peer_id: &PeerId, data: Message) -> bool {
			let subscribers = self.stream.subscribers.lock().unwrap();
			for subscriber in subscribers.iter() {
				if let Err(e) =
					subscriber.unbounded_send(P2PEvent::Received(peer_id.to_base58(), data.clone()))
				{
					debug!("Failed to send message {:?}", e);
				}
			}
			true
		}

		fn broadcast(&self, data: Message) -> bool {
			let subscribers = self.stream.subscribers.lock().unwrap();
			for subscriber in subscribers.iter() {
				if let Err(e) =
					subscriber.unbounded_send(P2PEvent::Received("".to_string(), data.clone()))
				{
					debug!("Failed to send message {:?}", e);
				}
			}
			true
		}
	}

	impl Node {
		fn new() -> Self {
			let executor = Arc::new(TaskExecutor);
			let (core, stream) = RpcCore::new(executor);
			let mut io = jsonrpc_core::MetaIoHandler::default();
			let stream = Arc::new(stream);
			let messenger = Messenger {
				stream: stream.clone(),
			};
			let messenger = Arc::new(Mutex::new(messenger));
			let rpc = Rpc::new(messenger.clone(), Arc::new(core));
			io.extend_with(RpcApi::to_delegate(rpc));

			Node {
				peer_id: PeerId::random(),
				io,
				messenger,
				stream: stream.clone(),
			}
		}

		fn notify(&self, event: P2PEvent) {
			let subscribers = self.stream.subscribers.lock().unwrap();
			for subscriber in subscribers.iter() {
				if let Err(e) = subscriber.unbounded_send(event.clone()) {
					debug!("Failed to send message {:?}", e);
				}
			}
		}
	}

	fn setup_session() -> (
		sc_rpc::Metadata,
		jsonrpc_core::futures::sync::mpsc::Receiver<String>,
	) {
		let (tx, rx) = jsonrpc_core::futures::sync::mpsc::channel(2);
		let meta = sc_rpc::Metadata::new(tx);
		(meta, rx)
	}

	#[test]
	fn validator_id_to_peer_id() {
		let validator_id = "5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqxG8Js2";
		let expected_peer_id = "12D3KooWMxxmtYRoBr5yMGfXdunkZ3goE4fZsMuJJMRAm3UdySxg";
		let public = Public::from_str(validator_id).unwrap();
		let ed25519 = ed25519::PublicKey::decode(&public.0).unwrap();
		let peer_id = PeerId::from_public_key(PublicKey::Ed25519(ed25519));
		let bs58 = peer_id.to_base58();
		assert_eq!(bs58, expected_peer_id);
	}

	#[test]
	fn subscribe_and_unsubscribe() {
		let node = Node::new();

		let (meta, _) = setup_session();

		let sub_request = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_subscribeNotifications",
			"params": [],
			"id": 1,
		});

		let resp = node
			.io
			.handle_request_sync(&sub_request.to_string(), meta.clone());
		let resp: Output = serde_json::from_str(&resp.unwrap()).unwrap();

		let sub_id = match resp {
			Output::Success(success) => success.result,
			_ => panic!(),
		};

		let unsub_req = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_unsubscribeNotifications",
			"params": [sub_id],
			"id": 1,
		});

		assert_eq!(
			node.io
				.handle_request_sync(&unsub_req.to_string(), meta.clone()),
			Some(r#"{"jsonrpc":"2.0","result":true,"id":1}"#.into()),
		);

		assert_eq!(
			node.io.handle_request_sync(&unsub_req.to_string(), meta),
			Some(r#"{"jsonrpc":"2.0","result":false,"id":1}"#.into()),
		);
	}

	#[test]
	fn send_message() {
		let node = Node::new();

		let validator_id = "5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqxG8Js2";

		let request = json!({
			"jsonrpc": "2.0",
			"method": "p2p_send",
			"params": [validator_id, "hello"],
			"id": 1,
		});

		let meta = sc_rpc::Metadata::default();
		assert_eq!(
			node.io.handle_request_sync(&request.to_string(), meta),
			Some("{\"jsonrpc\":\"2.0\",\"result\":200,\"id\":1}".to_string())
		);
	}

	#[test]
	fn broadcast_message() {
		let node = Node::new();

		let request = json!({
			"jsonrpc": "2.0",
			"method": "p2p_broadcast",
			"params": ["hello"],
			"id": 1,
		});

		let meta = sc_rpc::Metadata::default();
		assert_eq!(
			node.io.handle_request_sync(&request.to_string(), meta),
			Some("{\"jsonrpc\":\"2.0\",\"result\":200,\"id\":1}".to_string())
		);
	}

	#[test]
	fn subscribe_and_listen_for_messages() {
		let mut p2p = P2P::new();
		let peer_id = p2p.create_node();
		let node = p2p.get_node(&peer_id).unwrap();
		let (meta, receiver) = setup_session();

		let sub_request = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_subscribeNotifications",
			"params": [],
			"id": 1,
		});

		let resp = node
			.io
			.handle_request_sync(&sub_request.to_string(), meta.clone());
		let mut resp: serde_json::Value = serde_json::from_str(&resp.unwrap()).unwrap();
		let sub_id: String = serde_json::from_value(resp["result"].take()).unwrap();

		// Simulate a message being received from the peer
		let message: Message = vec![1, 2, 3];
		p2p.send_message(&peer_id, message.clone());

		// We should get a notification of this event
		let recv = receiver.take(1).wait().flatten().collect::<Vec<_>>();
		let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
		let mut json_map = match recv.params {
			Params::Map(json_map) => json_map,
			_ => panic!(),
		};

		let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
		let recv_message: P2PEvent = serde_json::from_value(json_map["result"].take()).unwrap();
		assert_eq!(recv.method, "cf_p2p_notifications");
		assert_eq!(recv_sub_id, sub_id);

		match recv_message {
			P2PEvent::Received(_, recv_message) => {
				assert_eq!(recv_message, message);
			}
			_ => panic!(),
		}
	}

	#[test]
	fn subscribe_and_listen_for_broadcast() {
		// Create a node and subscribe to it
		let mut p2p = P2P::new();
		let peer_id = p2p.create_node();
		let node = p2p.get_node(&peer_id).unwrap();

		let (meta, receiver) = setup_session();
		let sub_request = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_subscribeNotifications",
			"params": [],
			"id": 1,
		});
		let resp = node
			.io
			.handle_request_sync(&sub_request.to_string(), meta.clone());
		let mut resp: serde_json::Value = serde_json::from_str(&resp.unwrap()).unwrap();
		let sub_id: String = serde_json::from_value(resp["result"].take()).unwrap();

		// Create another node and subscribe to it
		let peer_id_1 = p2p.create_node();
		let node_1 = p2p.get_node(&peer_id_1).unwrap();
		let (meta_1, receiver_1) = setup_session();
		let sub_request_1 = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_subscribeNotifications",
			"params": [],
			"id": 1,
		});
		let resp_1 = node_1
			.io
			.handle_request_sync(&sub_request_1.to_string(), meta_1.clone());
		let mut resp_1: serde_json::Value = serde_json::from_str(&resp_1.unwrap()).unwrap();
		let sub_id_1: String = serde_json::from_value(resp_1["result"].take()).unwrap();

		// Simulate a message being received from the peer
		let message: Message = vec![1, 2, 3];
		p2p.broadcast(message.clone());

		// We should get a notification of this event
		let recv = receiver.take(1).wait().flatten().collect::<Vec<_>>();
		let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
		let mut json_map = match recv.params {
			Params::Map(json_map) => json_map,
			_ => panic!(),
		};

		let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
		let recv_message: P2PEvent = serde_json::from_value(json_map["result"].take()).unwrap();

		assert_eq!(recv.method, "cf_p2p_notifications");
		assert_eq!(recv_sub_id, sub_id);
		match recv_message {
			P2PEvent::Received(_, recv_message) => {
				assert_eq!(recv_message, message);
			}
			_ => panic!(),
		}

		let recv = receiver_1.take(1).wait().flatten().collect::<Vec<_>>();
		let recv: Notification = serde_json::from_str(&recv[0]).unwrap();
		let mut json_map = match recv.params {
			Params::Map(json_map) => json_map,
			_ => panic!(),
		};

		let recv_sub_id: String = serde_json::from_value(json_map["subscription"].take()).unwrap();
		let recv_message: P2PEvent = serde_json::from_value(json_map["result"].take()).unwrap();

		assert_eq!(recv.method, "cf_p2p_notifications");
		assert_eq!(recv_sub_id, sub_id_1);
		match recv_message {
			P2PEvent::Received(_, recv_message) => {
				assert_eq!(recv_message, message);
			}
			_ => panic!(),
		}
	}

	#[test]
	fn connect_disconnect_peer() {
		let mut p2p = P2P::new();
		let peer_id = p2p.create_node();
		let node = p2p.get_node(&peer_id).unwrap();
		let (meta, receiver) = setup_session();

		let sub_request = json!({
			"jsonrpc": "2.0",
			"method": "cf_p2p_subscribeNotifications",
			"params": [],
			"id": 1,
		});

		let resp = node
			.io
			.handle_request_sync(&sub_request.to_string(), meta.clone());
		let mut resp: serde_json::Value = serde_json::from_str(&resp.unwrap()).unwrap();
		let sub_id: String = serde_json::from_value(resp["result"].take()).unwrap();

		node.notify(P2PEvent::PeerConnected(peer_id.to_base58()));
		node.notify(P2PEvent::PeerDisconnected(peer_id.to_base58()));

		// We should get a notification for the two events
		let recv = receiver.take(2).wait().flatten().collect::<Vec<_>>();
		let mut events = vec![];

		for v in recv {
			let recv: Notification = serde_json::from_str(&v).unwrap();
			let mut json_map = match recv.params {
				Params::Map(json_map) => json_map,
				_ => panic!(),
			};

			let recv_sub_id: String =
				serde_json::from_value(json_map["subscription"].take()).unwrap();
			let recv_message: P2PEvent = serde_json::from_value(json_map["result"].take()).unwrap();
			assert_eq!(recv.method, "cf_p2p_notifications");
			assert_eq!(recv_sub_id, sub_id);
			events.push(recv_message);
		}

		assert_eq!(events.len(), 2);

		match events[0].clone() {
			P2PEvent::PeerConnected(recv_peer_id) => {
				assert_eq!(recv_peer_id, peer_id.to_base58());
			}
			_ => panic!(),
		}
		match events[1].clone() {
			P2PEvent::PeerDisconnected(recv_peer_id) => {
				assert_eq!(recv_peer_id, peer_id.to_base58());
			}
			_ => panic!(),
		}
	}
}
