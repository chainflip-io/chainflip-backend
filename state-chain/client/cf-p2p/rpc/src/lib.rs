pub mod p2p_serde;
use cf_p2p::{AccountId, MessagingCommand, NetworkObserver, RawMessage};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{StreamExt, TryStreamExt};
pub use gen_client::Client as P2PRpcClient;
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
pub struct AccountIdBs58(#[serde(with = "p2p_serde::bs58_fixed_size")] pub [u8; 32]);

impl From<AccountIdBs58> for AccountId {
	fn from(id: AccountIdBs58) -> Self {
		Self(id.0)
	}
}

impl From<AccountId> for AccountIdBs58 {
	fn from(id: AccountId) -> Self {
		Self(id.0)
	}
}

impl std::fmt::Display for AccountIdBs58 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", bs58::encode(&self.0).into_string())
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBs58(#[serde(with = "p2p_serde::bs58_vec")] pub Vec<u8>);

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

impl std::fmt::Display for MessageBs58 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", bs58::encode(&self.0).into_string())
	}
}

#[rpc]
pub trait RpcApi {
	/// RPC Metadata
	type Metadata;

	/// Identify yourself to the network.
	#[rpc(name = "p2p_self_identify")]
	fn self_identify(&self, validator_id: AccountIdBs58) -> Result<u64>;

	/// Send a message to validator id returning a HTTP status code
	#[rpc(name = "p2p_send")]
	fn send(&self, validator_id: AccountIdBs58, message: MessageBs58) -> Result<u64>;

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
pub struct P2PStream {
	subscribers: Arc<Mutex<Vec<UnboundedSender<P2PEvent>>>>,
}

impl P2PStream {
	fn new() -> Self {
		let subscribers = Arc::new(Mutex::new(vec![]));
		P2PStream { subscribers }
	}

	/// A new subscriber to be notified on upcoming events
	fn subscribe(&self) -> UnboundedReceiver<P2PEvent> {
		let (tx, rx) = unbounded();
		self.subscribers.lock().unwrap().push(tx);
		rx
	}
}

/// Our core bridge between p2p events and our RPC subscribers
pub struct RpcCore {
	stream: P2PStream,
	manager: SubscriptionManager,
}

/// Protocol errors notified via the subscription stream.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum P2pError {
	/// The recipient of a message could not be found on the network.
	UnknownRecipient(AccountIdBs58),
	/// This node can't send messages until it identifies itself to the network.
	Unidentified,
	/// Empty messages are not allowed.
	EmptyMessage,
	/// The node attempted to identify itself more than once.
	AlreadyIdentified(AccountIdBs58),
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
	MessageReceived(AccountIdBs58, MessageBs58),
	/// A new validator has cconnected and identified itself to the network.
	ValidatorConnected(AccountIdBs58),
	/// A validator has disconnected from the network.
	ValidatorDisconnected(AccountIdBs58),
	/// Errors.
	Error(P2pError),
}

impl RpcCore {
	pub fn new<E>(executor: Arc<E>) -> (Self, P2PStream)
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
	fn new_validator(&self, validator_id: &AccountId) {
		self.notify(P2PEvent::ValidatorConnected((*validator_id).into()));
	}

	fn disconnected(&self, validator_id: &AccountId) {
		self.notify(P2PEvent::ValidatorDisconnected((*validator_id).into()));
	}

	fn received(&self, validator_id: &AccountId, message: RawMessage) {
		self.notify(P2PEvent::MessageReceived(
			(*validator_id).into(),
			message.into(),
		));
	}

	fn unknown_recipient(&self, recipient_id: &AccountId) {
		self.notify(P2pError::UnknownRecipient((*recipient_id).into()).into());
	}

	fn unidentified_node(&self) {
		self.notify(P2pError::Unidentified.into());
	}

	fn empty_message(&self) {
		self.notify(P2pError::EmptyMessage.into());
	}

	fn already_identified(&self, existing_id: &AccountId) {
		self.notify(P2pError::AlreadyIdentified((*existing_id).into()).into());
	}
}

/// The RPC bridge and API
pub struct Rpc {
	core: Arc<RpcCore>,
	rpc_command_sender: Arc<UnboundedSender<MessagingCommand>>,
}

impl Rpc {
	pub fn new(
		rpc_command_sender: Arc<UnboundedSender<MessagingCommand>>,
		core: Arc<RpcCore>
	) -> Self {
		Rpc { rpc_command_sender, core }
	}

	fn messaging_command(&self, command : MessagingCommand) -> Result<u64> {
		match self.rpc_command_sender.unbounded_send(command) {
			Ok(()) => Ok(200),
			Err(error) => Err({
				let mut e = Error::internal_error();
				e.message = format!("{}", error);
				e
			})
		}
	}
}

/// Impl of the `RpcApi` - send, broadcast and subscribe to notifications
impl RpcApi for Rpc {
	type Metadata = sc_rpc::Metadata;

	fn self_identify(&self, validator_id: AccountIdBs58) -> Result<u64> {
		self.messaging_command(MessagingCommand::Identify(validator_id.into()))
	}

	fn send(&self, validator_id: AccountIdBs58, message: MessageBs58) -> Result<u64> {
		self.messaging_command(MessagingCommand::Send(validator_id.into(), message.into()))
	}

	fn broadcast(&self, message: MessageBs58) -> Result<u64> {
		self.messaging_command(MessagingCommand::BroadcastAll(message.into()))
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
