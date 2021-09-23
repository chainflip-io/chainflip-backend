//! Chainflip P2P layer.
//!
//! Provides an interface to substrate's peer-to-peer networking layer/
//!
//! How it works at a high level:
//!
//! The [NetworkBridge] is a `Future` that is passed to substrate's top-level task executor. The executor drives the
//! future, which reacts to:
//! 1. [MessagingCommand]s from the local node (passed in via the rpc layer that sits on top of this).
//! 2. [Event] notifications from the network.
//!
//! The [NetworkBridge] implementation relays relevant [Event]s (any event that is handled by our
//!   [protocol](CHAINFLIP_P2P_PROTOCOL_NAME)) and all [MessagingCommand]s to methods in the [StateMachine].
//!
//! The [StateMachine] contains the core protocol methods. The local node is notified of events via the
//! [NetworkObserver] trait. Outgoing messages can be sent to the network via the [PeerNetwork] trait. The default
//! implementation of [NetworkObserver] is the rpc server so that clients can be notified. The default implementation of
//! [PeerNetwork] is [NetworkService], which is substrate's `libp2p`-based network implementation.
pub mod p2p_serde;
pub use gen_client::Client as P2PRpcClient;

use futures::stream::Fuse;
use core::iter;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{StreamExt, TryStreamExt};
use sc_network::{multiaddr, Event, ExHashT, NetworkService, PeerId};
use serde::{self, Deserialize, Serialize};
use sp_runtime::traits::Block as BlockT;
use std::borrow::Cow;
use std::collections::HashMap;
use std::pin::Pin;
use jsonrpc_core::futures::Sink;
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use jsonrpc_core::Error;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{manager::SubscriptionManager, typed::Subscriber, SubscriptionId};
use log::{debug, warn};
use std::marker::Send;
use sp_runtime::sp_std::sync::{Arc, Mutex};

// TODO: This is duplicated in the CFE, can we just use one of these?
/// The type of validator id expected by the p2p layer, uses standard serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub [u8; 32]);

/// A wrapper around a byte buffer containing some opaque message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMessage(pub Vec<u8>);

/// The protocol has two message types, `Identify` and `Message`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum P2PMessage {
	SelfIdentify(AccountId),
	Message(RawMessage),
}

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
	) -> Result<bool>;
}

/// Our core bridge between p2p events and our RPC subscribers
pub struct RpcCore {
	subscribers: Mutex<Vec<UnboundedSender<P2PEvent>>>,
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
	pub fn new<E>(executor: Arc<E>) -> Self
	where
		E: Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>> + Send + Sync + 'static,
	{
		RpcCore {
			subscribers : Default::default(),
			manager: SubscriptionManager::new(executor),
		}
	}

	/// A new subscriber to be notified on upcoming events
	fn subscribe(&self) -> UnboundedReceiver<P2PEvent> {
		let (tx, rx) = unbounded();
		self.subscribers.lock().unwrap().push(tx);
		rx
	}

	/// Notify to our subscribers
	fn notify(&self, event: P2PEvent) {
		let subscribers = self.subscribers.lock().unwrap();
		for subscriber in subscribers.iter() {
			if let Err(e) = subscriber.unbounded_send(event.clone()) {
				debug!("Failed to send message: {:?}", e);
			}
		}
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

	fn messaging_command(&self, command : MessagingCommand) -> jsonrpc_core::Result<u64> {
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
		self.messaging_command(MessagingCommand::SelfIdentify(validator_id.into()))
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
	) -> Result<bool> {
		Ok(self.core.manager.cancel(id))
	}
}

/// The identifier for our protocol, required to distinguish it from other protocols running on the substrate p2p
/// network.
pub const CHAINFLIP_P2P_PROTOCOL_NAME: Cow<str> = Cow::Borrowed("/chainflip-protocol");

/// Required by substrate to register and configure the protocol.
pub fn p2p_peers_set_config() -> sc_network::config::NonDefaultSetConfig {
	sc_network::config::NonDefaultSetConfig {
		notifications_protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
		// Notifications reach ~256kiB in size at the time of writing on Kusama and Polkadot.
		max_notification_size: 1024 * 1024,
		set_config: sc_network::config::SetConfig {
			in_peers: 0,
			out_peers: 0,
			reserved_nodes: Vec::new(),
			non_reserved_mode: sc_network::config::NonReservedPeerMode::Deny,
		},
	}
}

/// An abstration of the underlying network of peers.
pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peer(&self, who: PeerId);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peer(&self, who: PeerId);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, message: Vec<u8>);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>>;
}

/// An implementation of [PeerNetwork] using substrate's libp2p-based `NetworkService`.
impl<B: BlockT, H: ExHashT> PeerNetwork for NetworkService<B, H> {
	fn reserve_peer(&self, who: PeerId) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result =
			self.add_peers_to_reserved_set(CHAINFLIP_P2P_PROTOCOL_NAME, iter::once(addr).collect());
		if let Err(err) = result {
			log::error!(target: "p2p", "add_set_reserved failed: {}", err);
		}
	}

	fn remove_reserved_peer(&self, who: PeerId) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result = self.remove_peers_from_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			iter::once(addr).collect(),
		);
		if let Err(err) = result {
			log::error!(target: "p2p", "remove_set_reserved failed: {}", err);
		}
	}

	fn write_notification(&self, target: PeerId, message: Vec<u8>) {
		self.write_notification(target, CHAINFLIP_P2P_PROTOCOL_NAME, message);
	}

	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
		Box::pin(self.event_stream("network-chainflip"))
	}
}

/// Defines the logic for processing network events and commands from this node.
///
/// ## ID management
///
/// Peers must identify themselves by their `AccountId` otherwise they will be unable to send
/// messages.
/// Likewise, any messages received from peers that have not identified themselves will be dropped.
struct StateMachine<Network: PeerNetwork> {
	/// A reference to a RpcCore
	rpc_core: Arc<RpcCore>,
	/// The peer to peer network
	network: Arc<Network>,
	/// PeerIds with the corresponding AccountId, if available.
	peer_to_validator: HashMap<PeerId, Option<AccountId>>,
	/// AccountIds mapped to corresponding PeerIds.
	validator_to_peer: HashMap<AccountId, PeerId>,
	/// Our own AccountId
	local_validator_id: Option<AccountId>,
}

const EXPECTED_PEER_COUNT: usize = 300;

impl<Network> StateMachine<Network>
where
	Network: PeerNetwork,
{
	pub fn new(rpc_core: Arc<RpcCore>, network: Arc<Network>) -> Self {
		StateMachine {
			rpc_core,
			network,
			peer_to_validator: HashMap::with_capacity(EXPECTED_PEER_COUNT),
			validator_to_peer: HashMap::with_capacity(EXPECTED_PEER_COUNT),
			local_validator_id: None,
		}
	}

	/// A new peer has arrived, insert into our internal list and identify ourselves if we can.
	pub fn new_peer(&mut self, peer_id: &PeerId) {
		self.peer_to_validator.insert(peer_id.clone(), None);
		if let Some(validator_id) = self.local_validator_id {
			self.send_identification(*peer_id, validator_id);
		}
	}

	/// A peer has identified itself. Register the validator Id and notify the observer.
	fn register_identification(&mut self, peer_id: &PeerId, validator_id: AccountId) {
		if let Some(entry) = self.peer_to_validator.get_mut(peer_id) {
			if entry.is_none() {
				*entry = Some(validator_id);
				self.validator_to_peer.insert(validator_id, peer_id.clone());
				self.rpc_core.notify(P2PEvent::ValidatorConnected(validator_id.into()));
			} else {
				log::warn!(
					"Received a duplicate identification {:?} for peer {:?}",
					validator_id,
					peer_id
				);
			}
		} else {
			log::error!(
				"An unknown peer {:?} identified itself as {:?}",
				peer_id,
				validator_id
			);
		}
	}

	/// A peer has disconnected, remove from our internal lookups and notify the observer.
	pub fn disconnected(&mut self, peer_id: &PeerId) {
		if let Some(Some(validator_id)) = self.peer_to_validator.remove(peer_id) {
			if let Some(_) = self.validator_to_peer.remove(&validator_id) {
				self.rpc_core.notify(P2PEvent::ValidatorDisconnected(validator_id.into()));
			}
		}
	}

	/// Notify the observer, if the validator id of the peer is known.
	fn maybe_notify_observer(&self, peer_id: &PeerId, message: RawMessage) {
		if let Some(Some(validator_id)) = self.peer_to_validator.get(peer_id) {
			self.rpc_core.notify(P2PEvent::MessageReceived((*validator_id).into(), message.into()));
		} else {
			log::error!("Dropping message from unidentified peer {:?}", peer_id);
		}
	}

	/// Messages received from peer_id, notify observer as long as the corresponding validator_id is known.
	pub fn received(&mut self, peer_id: &PeerId, messages: Vec<P2PMessage>) {
		if !self.peer_to_validator.contains_key(peer_id) {
			log::error!("Dropping message from unknown peer {:?}", peer_id);
			return;
		}

		for message in messages {
			match message {
				P2PMessage::SelfIdentify(validator_id) => {
					self.register_identification(peer_id, validator_id);
				}
				P2PMessage::Message(raw_message) => {
					self.maybe_notify_observer(peer_id, raw_message);
				}
			}
		}
	}

	/// Identify ourselves to the network.
	pub fn self_identify(&mut self, validator_id: AccountId) {
		if let Some(existing_id) = self.local_validator_id {
			self.rpc_core.notify(P2PEvent::Error(P2pError::AlreadyIdentified(existing_id.into())));
			return;
		}
		self.local_validator_id = Some(validator_id);
		for peer_id in self.peer_to_validator.keys() {
			self.send_identification(*peer_id, validator_id);
		}
	}

	/// Identify ourselves to a peer on the network.
	fn send_identification(&self, peer_id: PeerId, validator_id: AccountId) {
		self.encode_and_send(peer_id, P2PMessage::SelfIdentify(validator_id));
	}

	/// Send message to peer, this will fail silently if peer isn't in our peer list or if the message
	/// is empty.
	pub fn send_message(&self, validator_id: AccountId, message: RawMessage) {
		if self.notify_invalid(&message) {
			return;
		}

		if let Some(peer_id) = self.validator_to_peer.get(&validator_id) {
			self.encode_and_send(*peer_id, P2PMessage::Message(message));
		} else {
			self.rpc_core.notify(P2PEvent::Error(P2pError::UnknownRecipient(validator_id.into())));
		}
	}

	/// Broadcast & to a specific list of peers on the network, this will fail silently if the message is empty.
	pub fn broadcast(&self, validators: Vec<AccountId>, message: RawMessage) {
		if self.notify_invalid(&message) {
			return;
		}

		for validator_id in validators {
			self.send_message(validator_id, message.clone());
		}
	}

	/// Broadcast message to all known validators on the network, this will fail silently if the message is empty.
	pub fn broadcast_all(&self, message: RawMessage) {
		if self.notify_invalid(&message) {
			return;
		}

		for peer_id in self.validator_to_peer.values() {
			self.encode_and_send(*peer_id, P2PMessage::Message(message.clone()));
		}
	}

	/// Encodes the message using bincode and sends it over the network.
	fn encode_and_send(&self, peer_id: PeerId, message: P2PMessage) {
		bincode::serialize(&message)
			.map(|bytes| {
				self.network.write_notification(peer_id, bytes);
			})
			.unwrap_or_else(|err| {
				log::error!("Error while serializing p2p protocol message {}", err);
			})
	}

	/// If the message is invalid, or the local node is unidentified, notifies the observer and
	/// returns true. Otherwise returns false.
	fn notify_invalid(&self, message: &RawMessage) -> bool {
		if message.0.is_empty() {
			self.rpc_core.notify(P2PEvent::Error(P2pError::EmptyMessage));
			return true;
		}
		if self.local_validator_id.is_none() {
			self.rpc_core.notify(P2PEvent::Error(P2pError::Unidentified));
			return true;
		}
		false
	}

	pub fn try_decode(&self, bytes: &[u8]) -> anyhow::Result<P2PMessage> {
		Ok(bincode::deserialize(bytes)?)
	}
}

/// The entry point. The network bridge implements a `Future` that can be polled to advance the
/// state of the network by polling (a) its command_receiver for messages to send and (b)
/// the underlying network for notifications from other peers.
///
/// The `StateMachine` implements the logic of how to process commands and how to react to
/// network notifications.
pub struct NetworkBridge<Network: PeerNetwork> {
	state_machine: StateMachine<Network>,
	network_event_stream: Fuse<Pin<Box<dyn futures::Stream<Item = Event> + Send>>>,
	command_receiver: UnboundedReceiver<MessagingCommand>,
}

impl<Network: PeerNetwork> NetworkBridge<Network> {
	pub fn new(
		rpc_core: Arc<RpcCore>,
		p2p_network: Arc<Network>,
	) -> (Self, Arc<UnboundedSender<MessagingCommand>>) {
		let state_machine = StateMachine::new(rpc_core, p2p_network.clone());
		let network_event_stream = p2p_network.event_stream().fuse();
		let (command_sender, command_receiver) = unbounded();
		(
			NetworkBridge {
				state_machine,
				network_event_stream,
				command_receiver,
			},
			Arc::new(command_sender),
		)
	}
}

/// Commands that can be sent to the `NetworkBridge`. Each should correspond to a function in the bridge's
/// `StateMachine`.
pub enum MessagingCommand {
	SelfIdentify(AccountId),
	Send(AccountId, RawMessage),
	Broadcast(Vec<AccountId>, RawMessage),
	BroadcastAll(RawMessage),
}

impl<N> NetworkBridge<N>
where
	N: PeerNetwork,
{
	pub async fn start(mut self) {
		loop {
			futures::select!(
				option_command = self.command_receiver.next() => {
					match option_command {
						Some(cmd) => {
							match cmd {
								MessagingCommand::Send(validator_id, msg) => {
									self.state_machine.send_message(validator_id, msg);
								}
								MessagingCommand::Broadcast(validators, msg) => {
									self.state_machine.broadcast(validators, msg);
								}
								MessagingCommand::BroadcastAll(msg) => {
									self.state_machine.broadcast_all(msg);
								}
								MessagingCommand::SelfIdentify(validator_id) => {
									self.state_machine.self_identify(validator_id);
								}
							}
						},
						None => break
					}
				},
				option_event = self.network_event_stream.next() => {
					match option_event {
						Some(event) => {
							match event {
								Event::SyncConnected { remote } => {
									self.state_machine.network.reserve_peer(remote);
								}
								Event::SyncDisconnected { remote } => {
									self.state_machine.network.remove_reserved_peer(remote);
								}
								Event::NotificationStreamOpened {
									remote,
									protocol,
									role: _,
								} => {
									if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
										self.state_machine.new_peer(&remote);
									}
								}
								Event::NotificationStreamClosed { remote, protocol } => {
									if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
										self.state_machine.disconnected(&remote);
									}
								}
								Event::NotificationsReceived { remote, messages } => {
									if !messages.is_empty() {
										let messages: Vec<P2PMessage> =
											messages
												.into_iter()
												.filter_map(|(protocol, data)| {
													if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
														self.state_machine
															.try_decode(data.as_ref())
															.map_err(|err| {
																log::error!("Error deserializing protocol message: {}", err);
															})
															.ok()
													} else {
														None
													}
												})
												.collect();
		
										self.state_machine.received(&remote, messages);
									}
								}
								Event::Dht(_) => {}
							}
						},
						None => break
					}
				},
			);
		}
	}
}
