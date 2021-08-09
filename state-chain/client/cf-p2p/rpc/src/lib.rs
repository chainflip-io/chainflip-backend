pub mod p2p_serde;
use cf_p2p::{NetworkObserver, P2pMessaging, RawMessage, ValidatorId};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{StreamExt, TryStreamExt};
use jsonrpc_core::futures::Sink;
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use jsonrpc_core::Error;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{manager::SubscriptionManager, typed::Subscriber, SubscriptionId};
use log::{debug, warn};
use serde::{self, Deserialize, Serialize};
use std::marker::Send;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorIdBs58(#[serde(with = "p2p_serde::bs58_fixed_size")] [u8; 32]);

impl From<ValidatorIdBs58> for ValidatorId {
	fn from(id: ValidatorIdBs58) -> Self {
		Self(id.0)
	}
}

impl From<ValidatorId> for ValidatorIdBs58 {
	fn from(id: ValidatorId) -> Self {
		Self(id.0)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBs58(#[serde(with = "p2p_serde::bs58_vec")] Vec<u8>);

impl From<MessageBs58> for RawMessage {
	fn from(msg: MessageBs58) -> Self {
		Self(msg.0)
	}
}

impl From<RawMessage> for MessageBs58 {
	fn from(msg: RawMessage) -> Self {
		Self(msg.0)
	}
}

#[rpc]
pub trait RpcApi {
	/// RPC Metadata
	type Metadata;

	/// Identify yourself to the network.
	#[rpc(name = "p2p_identify")]
	fn identify(&self, validator_id: ValidatorIdBs58) -> Result<u64>;

	/// Send a message to validator id returning a HTTP status code
	#[rpc(name = "p2p_send")]
	fn send(&self, validator_id: ValidatorIdBs58, message: MessageBs58) -> Result<u64>;

	/// Broadcast a message to the p2p network returning a HTTP status code
	#[rpc(name = "p2p_broadcast")]
	fn broadcast(&self, message: MessageBs58) -> Result<u64>;

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
#[derive(Clone)]
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
type EventStream = P2PStream<P2PEvent>;

/// Our core bridge between p2p events and our RPC subscribers
pub struct RpcCore {
	stream: EventStream,
	manager: SubscriptionManager,
}

/// Protocol errors notified via the subscription stream.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum P2pError {
	/// The recipient of a message could not be found on the network.
	UnknownRecipient(ValidatorIdBs58),
	/// This node can't send messages until it identifies itself to the network.
	Unidentified,
	/// Empty messages are not allowed.
	EmptyMessage,
	/// The node attempted to identify itself more than once.
	AlreadyIdentified(ValidatorIdBs58),
}

impl From<P2pError> for P2PEvent {
	fn from(err: P2pError) -> Self {
		P2PEvent::Error(err)
	}
}

/// Events available via the subscription stream.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum P2PEvent {
	/// A message has been received from another validator.
	MessageReceived(ValidatorIdBs58, MessageBs58),
	/// A new validator has cconnected and identified itself to the network.
	ValidatorConnected(ValidatorIdBs58),
	/// A validator has disconnected from the network.
	ValidatorDisconnected(ValidatorIdBs58),
	/// Errors.
	Error(P2pError),
}

impl RpcCore {
	pub fn new<E>(executor: Arc<E>) -> (Self, EventStream)
	where
		E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
	{
		let stream = P2PStream::new();
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
	fn new_validator(&self, validator_id: &ValidatorId) {
		self.notify(P2PEvent::ValidatorConnected((*validator_id).into()));
	}

	fn disconnected(&self, validator_id: &ValidatorId) {
		self.notify(P2PEvent::ValidatorDisconnected((*validator_id).into()));
	}

	fn received(&self, validator_id: &ValidatorId, message: RawMessage) {
		self.notify(P2PEvent::MessageReceived(
			(*validator_id).into(),
			message.into(),
		));
	}

	fn unknown_recipient(&self, recipient_id: &ValidatorId) {
		self.notify(P2pError::UnknownRecipient((*recipient_id).into()).into());
	}

	fn unidentified_node(&self) {
		self.notify(P2pError::Unidentified.into());
	}

	fn empty_message(&self) {
		self.notify(P2pError::EmptyMessage.into());
	}

	fn already_identified(&self, existing_id: &ValidatorId) {
		self.notify(P2pError::AlreadyIdentified((*existing_id).into()).into());
	}
}

/// The RPC bridge and API
pub struct Rpc<C: P2pMessaging> {
	core: Arc<RpcCore>,
	messaging: Arc<Mutex<C>>,
}

impl<C: P2pMessaging> Rpc<C> {
	pub fn new(messaging: Arc<Mutex<C>>, core: Arc<RpcCore>) -> Self {
		Rpc { messaging, core }
	}
}

/// Impl of the `RpcApi` - send, broadcast and subscribe to notifications
impl<C: P2pMessaging + Sync + Send + 'static> RpcApi for Rpc<C> {
	type Metadata = sc_rpc::Metadata;

	fn identify(&self, validator_id: ValidatorIdBs58) -> Result<u64> {
		self.messaging
			.lock()
			.unwrap()
			.identify(validator_id.into())
			.map_err(|_| Error::internal_error())
			.map(|_| 200)
	}

	fn send(&self, validator_id: ValidatorIdBs58, message: MessageBs58) -> Result<u64> {
		self.messaging
			.lock()
			.unwrap()
			.send_message(validator_id.into(), message.into())
			.map_err(|_| Error::internal_error())
			.map(|_| 200)
	}

	fn broadcast(&self, message: MessageBs58) -> Result<u64> {
		self.messaging
			.lock()
			.unwrap()
			.broadcast_all(message.into())
			.map_err(|_| Error::internal_error())
			.map(|_| 200)
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
	use sc_rpc::testing::TaskExecutor;
	use serde_json::json;

	/// A node on this test network
	struct Node {
		io: jsonrpc_core::MetaIoHandler<sc_rpc::Metadata>,
		event_stream: Arc<EventStream>,
	}

	impl Node {
		fn new() -> Self {
			let executor = Arc::new(TaskExecutor);
			let (core, event_stream) = RpcCore::new(executor);
			let mut io = jsonrpc_core::MetaIoHandler::default();
			let event_stream = Arc::new(event_stream);
			let (sender, _receiver) = cf_p2p::Sender::new();
			let messenger = Arc::new(Mutex::new(sender));
			let rpc = Rpc::new(messenger, Arc::new(core));
			io.extend_with(RpcApi::to_delegate(rpc));

			Node {
				io,
				event_stream: event_stream.clone(),
			}
		}

		fn notify(&self, event: P2PEvent) -> anyhow::Result<()> {
			let subscribers = self.event_stream.subscribers.lock().unwrap();
			for subscriber in subscribers.iter() {
				subscriber.unbounded_send(event.clone())?;
			}
			Ok(())
		}

		fn notify_message(&self, from: ValidatorId, data: RawMessage) -> anyhow::Result<()> {
			self.notify(P2PEvent::MessageReceived(from.into(), data.into()))
		}

		fn notify_identity(&self, who: ValidatorId) -> anyhow::Result<()> {
			self.notify(P2PEvent::ValidatorConnected(who.into()))
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

		let validator_id = "5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqx";
		let message = bs58::encode(b"hello").into_string();

		let request = json!({
			"jsonrpc": "2.0",
			"method": "p2p_send",
			"params": [validator_id, message],
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
		let message = bs58::encode(b"hello").into_string();

		let request = json!({
			"jsonrpc": "2.0",
			"method": "p2p_broadcast",
			"params": [message],
			"id": 1,
		});

		let meta = sc_rpc::Metadata::default();
		assert_eq!(
			node.io.handle_request_sync(&request.to_string(), meta),
			Some("{\"jsonrpc\":\"2.0\",\"result\":200,\"id\":1}".to_string())
		);
	}

	#[test]
	fn identify_message() {
		let node = Node::new();
		let validator_id = "5G9NWJ5P9uk7am24yCKeLZJqXWW6hjuMyRJDmw4ofqx";

		let request = json!({
			"jsonrpc": "2.0",
			"method": "p2p_identify",
			"params": [validator_id],
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
		let node = Node::new();
		let (meta, notifications_receiver) = setup_session();

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

		// Simulate messages being received from a peer
		let peer = ValidatorId([0xCF; 32]);
		let message = RawMessage(vec![1, 2, 3]);
		node.notify_identity(peer).unwrap();
		node.notify_message(peer, message.clone()).unwrap();

		// We should get notifications of these events
		let events = notifications_receiver
			.take(2)
			.wait()
			.map(|s| serde_json::from_str::<Notification>(s.unwrap().as_ref()).unwrap())
			.map(|notification| {
				assert_eq!(notification.method, "cf_p2p_notifications");

				let mut json_map = match notification.params {
					Params::Map(json_map) => json_map,
					_ => panic!(),
				};
				let recv_sub_id: String =
					serde_json::from_value(json_map["subscription"].take()).unwrap();
				assert_eq!(recv_sub_id, sub_id);

				serde_json::from_value(json_map["result"].take()).unwrap()
			})
			.collect::<Vec<P2PEvent>>();

		match events[0].clone() {
			P2PEvent::ValidatorConnected(id) => {
				assert_eq!(id, peer.into());
			}
			_ => panic!("Unexpected message type."),
		}
		match events[1].clone() {
			P2PEvent::MessageReceived(id, recv_message) => {
				assert_eq!(id, peer.into());
				assert_eq!(recv_message, message.into());
			}
			_ => panic!("Unexpected message type."),
		}
	}
}
