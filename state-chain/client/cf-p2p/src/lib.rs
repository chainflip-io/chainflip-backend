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

use anyhow::Result;
use core::iter;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{Future, Stream, StreamExt};
use sc_network::{multiaddr, Event, ExHashT, NetworkService, PeerId};
use serde::{Deserialize, Serialize};
use sp_runtime::sp_std::sync::Arc;
use sp_runtime::traits::Block as BlockT;
use std::borrow::Cow;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValidatorId(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMessage(pub Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ProtocolMessage {
	Identify(ValidatorId),
	Message(RawMessage),
}

pub const CHAINFLIP_P2P_PROTOCOL_NAME: Cow<str> = Cow::Borrowed("/chainflip-protocol");

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

pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peer(&self, who: PeerId);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peer(&self, who: PeerId);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, message: Vec<u8>);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
}

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

	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
		Box::pin(self.event_stream("network-chainflip"))
	}
}

/// A collection of callbacks for network events.
pub trait NetworkObserver {
	/// Called when a peer identifies itself to the network.
	fn new_validator(&self, validator_id: &ValidatorId);
	/// Called when a peer is disconnected.
	fn disconnected(&self, validator_id: &ValidatorId);
	/// Called when a message is received from some validator_id for this peer.
	fn received(&self, from: &ValidatorId, message: RawMessage);
	/// Called when a message could not be delivered because the recipient is unknown.
	fn unknown_recipient(&self, recipient_id: &ValidatorId);
	/// Called when a message is sent before identifying the node to the network.
	fn unidentified_node(&self);
	/// Empty messages are not allowed.
	fn empty_message(&self);
	/// A node cannot identify more than once.
	fn already_identified(&self, existing_id: &ValidatorId);
}

/// Defines the logic for processing network events and commands from this node.
///
/// ## ID management
///
/// Peers must identify themselves by their `ValidatorId` otherwise they will be unable to send
/// messages.
/// Likewise, any messages received from peers that have not identified themselves will be dropped.
struct StateMachine<Observer: NetworkObserver, Network: PeerNetwork> {
	/// A reference to a NetworkObserver
	observer: Arc<Observer>,
	/// The peer to peer network
	network: Arc<Network>,
	/// PeerIds with the corresponding ValidatorId, if available.
	peer_to_validator: HashMap<PeerId, Option<ValidatorId>>,
	/// ValidatorIds mapped to corresponding PeerIds.
	validator_to_peer: HashMap<ValidatorId, PeerId>,
	/// Our own ValidatorId
	local_validator_id: Option<ValidatorId>,
}

const EXPECTED_PEER_COUNT: usize = 300;

impl<Observer, Network> StateMachine<Observer, Network>
where
	Observer: NetworkObserver,
	Network: PeerNetwork,
{
	pub fn new(observer: Arc<Observer>, network: Arc<Network>) -> Self {
		StateMachine {
			observer,
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
	fn register_identification(&mut self, peer_id: &PeerId, validator_id: ValidatorId) {
		if let Some(entry) = self.peer_to_validator.get_mut(peer_id) {
			if entry.is_none() {
				*entry = Some(validator_id);
				self.validator_to_peer.insert(validator_id, peer_id.clone());
				self.observer.new_validator(&validator_id);
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
				self.observer.disconnected(&validator_id);
			}
		}
	}

	/// Notify the observer, if the validator id of the peer is known.
	fn maybe_notify_observer(&self, peer_id: &PeerId, message: RawMessage) {
		if let Some(Some(validator_id)) = self.peer_to_validator.get(peer_id) {
			self.observer.received(validator_id, message);
		} else {
			log::error!("Dropping message from unidentified peer {:?}", peer_id);
		}
	}

	/// Messages received from peer_id, notify observer as long as the corresponding validator_id is known.
	pub fn received(&mut self, peer_id: &PeerId, messages: Vec<ProtocolMessage>) {
		if !self.peer_to_validator.contains_key(peer_id) {
			log::error!("Dropping message from unknown peer {:?}", peer_id);
			return;
		}

		for message in messages {
			match message {
				ProtocolMessage::Identify(validator_id) => {
					self.register_identification(peer_id, validator_id);
				}
				ProtocolMessage::Message(raw_message) => {
					self.maybe_notify_observer(peer_id, raw_message);
				}
			}
		}
	}

	/// Identify ourselves to the network.
	pub fn identify(&mut self, validator_id: ValidatorId) {
		if let Some(existing_id) = self.local_validator_id {
			self.observer.already_identified(&existing_id);
			return;
		}
		self.local_validator_id = Some(validator_id);
		for peer_id in self.peer_to_validator.keys() {
			self.send_identification(*peer_id, validator_id);
		}
	}

	/// Identify ourselves to a peer on the network.
	fn send_identification(&self, peer_id: PeerId, validator_id: ValidatorId) {
		self.encode_and_send(peer_id, ProtocolMessage::Identify(validator_id));
	}

	/// Send message to peer, this will fail silently if peer isn't in our peer list or if the message
	/// is empty.
	pub fn send_message(&self, validator_id: ValidatorId, message: RawMessage) {
		if self.notify_invalid(&message) {
			return;
		}

		if let Some(peer_id) = self.validator_to_peer.get(&validator_id) {
			self.encode_and_send(*peer_id, ProtocolMessage::Message(message));
		} else {
			self.observer.unknown_recipient(&validator_id);
		}
	}

	/// Broadcast & to a specific list of peers on the network, this will fail silently if the message is empty.
	pub fn broadcast(&self, validators: Vec<ValidatorId>, message: RawMessage) {
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
			self.encode_and_send(*peer_id, ProtocolMessage::Message(message.clone()));
		}
	}

	/// Encodes the message using bincode and sends it over the network.
	fn encode_and_send(&self, peer_id: PeerId, message: ProtocolMessage) {
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
			self.observer.empty_message();
			return true;
		}
		if self.local_validator_id.is_none() {
			self.observer.unidentified_node();
			return true;
		}
		false
	}

	pub fn try_decode(&self, bytes: &[u8]) -> Result<ProtocolMessage> {
		Ok(bincode::deserialize(bytes)?)
	}
}

/// The entry point. The network bridge implements a `Future` that can be polled to advance the
/// state of the network by polling (a) its command_receiver for messages to send and (b)
/// the underlying network for notifications from other peers.
///
/// The `StateMachine` implements the logic of how to process commands and how to react to
/// network notifications.
pub struct NetworkBridge<Observer: NetworkObserver, Network: PeerNetwork> {
	state_machine: StateMachine<Observer, Network>,
	network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
	command_receiver: UnboundedReceiver<MessagingCommand>,
}

pub fn substrate_network_bridge<Observer: NetworkObserver, B: BlockT, H: ExHashT>(
	observer: Arc<Observer>,
	network: Arc<NetworkService<B, H>>,
) -> (
	NetworkBridge<Observer, NetworkService<B, H>>,
	Arc<Mutex<Sender>>,
) {
	NetworkBridge::new(observer, network)
}

impl<Observer: NetworkObserver, Network: PeerNetwork> NetworkBridge<Observer, Network> {
	pub(crate) fn new(
		observer: Arc<Observer>,
		p2p_network: Arc<Network>,
	) -> (Self, Arc<Mutex<Sender>>) {
		let state_machine = StateMachine::new(observer, p2p_network.clone());
		let network_event_stream = Box::pin(p2p_network.event_stream());
		let (sender, command_receiver) = Sender::new();
		let sender = Arc::new(Mutex::new(sender));
		(
			NetworkBridge {
				state_machine,
				network_event_stream,
				command_receiver,
			},
			sender,
		)
	}
}

/// Commands that can be sent to the `NetworkBridge`. Each should correspond to a function in the bridge's
/// `StateMachine`.
pub enum MessagingCommand {
	Identify(ValidatorId),
	Send(ValidatorId, RawMessage),
	Broadcast(Vec<ValidatorId>, RawMessage),
	BroadcastAll(RawMessage),
}

/// Messaging by sending directly or broadcasting
pub trait P2PMessaging {
	fn identify(&mut self, validator_id: ValidatorId) -> Result<()>;
	fn send_message(&mut self, validator_id: ValidatorId, data: RawMessage) -> Result<()>;
	fn broadcast(&self, validators: Vec<ValidatorId>, data: RawMessage) -> Result<()>;
	fn broadcast_all(&self, data: RawMessage) -> Result<()>;
}

/// A thin wrapper around an `UnboundedSender` channel. Messages pushed to this will be
/// relayed to the network.
pub struct Sender(UnboundedSender<MessagingCommand>);

impl Sender {
	pub fn new() -> (Self, UnboundedReceiver<MessagingCommand>) {
		let (tx, rx) = unbounded();
		(Self(tx), rx)
	}
}

impl P2PMessaging for Sender {
	fn identify(&mut self, validator_id: ValidatorId) -> Result<()> {
		self.0
			.unbounded_send(MessagingCommand::Identify(validator_id))?;
		Ok(())
	}

	fn send_message(&mut self, validator_id: ValidatorId, data: RawMessage) -> Result<()> {
		self.0
			.unbounded_send(MessagingCommand::Send(validator_id, data))?;
		Ok(())
	}

	fn broadcast(&self, validators: Vec<ValidatorId>, data: RawMessage) -> Result<()> {
		self.0
			.unbounded_send(MessagingCommand::Broadcast(validators, data))?;
		Ok(())
	}

	fn broadcast_all(&self, data: RawMessage) -> Result<()> {
		self.0
			.unbounded_send(MessagingCommand::BroadcastAll(data))?;
		Ok(())
	}
}

impl<O, N> Unpin for NetworkBridge<O, N>
where
	O: NetworkObserver,
	N: PeerNetwork,
{
}

/// `Future` for `NetworkBridge` - poll our outgoing messages and pass them to the `StateMachine` for sending
/// After which we poll the network for events and again back to the `StateMachine`
impl<Observer, Network> Future for NetworkBridge<Observer, Network>
where
	Observer: NetworkObserver,
	Network: PeerNetwork,
{
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
		let this = &mut *self;
		loop {
			match this.command_receiver.poll_next_unpin(cx) {
				Poll::Ready(Some(cmd)) => match cmd {
					MessagingCommand::Send(validator_id, msg) => {
						this.state_machine.send_message(validator_id, msg);
					}
					MessagingCommand::Broadcast(validators, msg) => {
						this.state_machine.broadcast(validators, msg);
					}
					MessagingCommand::BroadcastAll(msg) => {
						this.state_machine.broadcast_all(msg);
					}
					MessagingCommand::Identify(validator_id) => {
						this.state_machine.identify(validator_id);
					}
				},
				Poll::Ready(None) => return Poll::Ready(()),
				Poll::Pending => break,
			}
		}

		loop {
			match this.network_event_stream.poll_next_unpin(cx) {
				Poll::Ready(Some(event)) => {
					match event {
						Event::SyncConnected { remote } => {
							this.state_machine.network.reserve_peer(remote);
						}
						Event::SyncDisconnected { remote } => {
							this.state_machine.network.remove_reserved_peer(remote);
						}
						Event::NotificationStreamOpened {
							remote,
							protocol,
							role: _,
						} => {
							if protocol != CHAINFLIP_P2P_PROTOCOL_NAME {
								continue;
							}
							this.state_machine.new_peer(&remote);
						}
						Event::NotificationStreamClosed { remote, protocol } => {
							if protocol != CHAINFLIP_P2P_PROTOCOL_NAME {
								continue;
							}
							this.state_machine.disconnected(&remote);
						}
						Event::NotificationsReceived { remote, messages } => {
							if !messages.is_empty() {
								let messages: Vec<ProtocolMessage> =
									messages
										.into_iter()
										.filter_map(|(engine, data)| {
											if engine == CHAINFLIP_P2P_PROTOCOL_NAME {
												this.state_machine
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

								this.state_machine.received(&remote, messages);
							}
						}
						Event::Dht(_) => {}
					}
				}
				Poll::Ready(None) => return Poll::Ready(()),
				Poll::Pending => break,
			}
		}

		Poll::Pending
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::channel::mpsc::{unbounded, UnboundedSender};
	use futures::Stream;
	use sc_network::{Event, ObservedRole, PeerId};
	use std::cell::RefCell;
	use std::sync::Arc;

	struct TestNetwork {
		local_peer: PeerId,
		inner: RefCell<TestNetworkInner>,
	}

	impl TestNetwork {
		fn new(local_peer: PeerId) -> Self {
			Self {
				local_peer,
				inner: Default::default(),
			}
		}

		/// Add a sender to simulate messages sent to a remote peer.
		fn add_remote_peer(&self, peer_id: PeerId, sender: UnboundedSender<Event>) {
			self.inner.borrow_mut().peer_senders.insert(peer_id, sender);
		}

		/// Returns a sender that can be used to simulate network events. Requires `event_stream` to
		/// be called first to create the sender and receiver.
		fn get_local_event_sender(&self) -> UnboundedSender<Event> {
			self.inner
				.borrow()
				.local_sender
				.clone()
				.expect("no local sender, need to call `event_stream()` first")
		}

		/// Simulates an incoming `NotificationStreamOpened` message.
		fn open_notifications_from(&self, who: PeerId) {
			let event = Event::NotificationStreamOpened {
				remote: who,
				protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
				role: ObservedRole::Full,
			};
			self.notify_event(event);
		}

		fn notify_event(&self, event: Event) {
			self.get_local_event_sender().start_send(event).unwrap();
		}
	}

	#[derive(Clone, Default)]
	struct TestNetworkInner {
		// Allows us to write to some externally provided channel.
		peer_senders: HashMap<PeerId, UnboundedSender<Event>>,
		// Allows us to write to the local notification stream.
		local_sender: Option<UnboundedSender<Event>>,
	}

	impl PeerNetwork for TestNetwork {
		fn reserve_peer(&self, _who: PeerId) {}

		fn remove_reserved_peer(&self, _who: PeerId) {}

		fn write_notification(&self, who: PeerId, message: Vec<u8>) {
			let event = Event::NotificationsReceived {
				remote: self.local_peer,
				messages: vec![(CHAINFLIP_P2P_PROTOCOL_NAME, message.into())],
			};
			let mut sender = self.inner.borrow().peer_senders.get(&who).cloned().unwrap();
			sender.start_send(event).unwrap();
		}

		fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
			let (tx, rx) = unbounded();
			self.inner.borrow_mut().local_sender = Some(tx);

			Box::pin(rx)
		}
	}

	#[derive(Default)]
	struct MockObserver {
		pub inner: RefCell<MockObserverInner>,
	}

	#[derive(Default)]
	struct MockObserverInner {
		pub new_peers: Vec<ValidatorId>,
		pub disconnected_peers: Vec<ValidatorId>,
		pub messages_received: Vec<(ValidatorId, RawMessage)>,
		pub unknown_recipients: Vec<ValidatorId>,
		pub unidentified_node: Vec<()>,
		pub empty_message: Vec<()>,
		pub already_identified: Vec<ValidatorId>,
	}

	impl NetworkObserver for MockObserver {
		fn new_validator(&self, validator_id: &ValidatorId) {
			self.inner.borrow_mut().new_peers.push(*validator_id);
		}

		fn disconnected(&self, validator_id: &ValidatorId) {
			self.inner
				.borrow_mut()
				.disconnected_peers
				.push(*validator_id);
		}

		fn received(&self, validator_id: &ValidatorId, message: RawMessage) {
			self.inner
				.borrow_mut()
				.messages_received
				.push((*validator_id, message));
		}

		fn unknown_recipient(&self, recipient_id: &ValidatorId) {
			self.inner
				.borrow_mut()
				.unknown_recipients
				.push(*recipient_id);
		}

		fn unidentified_node(&self) {
			self.inner.borrow_mut().unidentified_node.push(());
		}

		fn empty_message(&self) {
			self.inner.borrow_mut().empty_message.push(());
		}

		fn already_identified(&self, existing_id: &ValidatorId) {
			self.inner
				.borrow_mut()
				.already_identified
				.push(*existing_id);
		}
	}

	#[test]
	fn test_state_machine() {
		let local_peer = PeerId::random();
		let local_validator_id = ValidatorId([0xCF; 32]);
		let remote_peer = PeerId::random();
		let remote_validator_id = ValidatorId([0xAB; 32]);
		let hello = RawMessage(b"hello".to_vec());

		let observer = Arc::new(MockObserver::default());
		let network = Arc::new(TestNetwork::new(local_peer));
		let mut sm = StateMachine::new(observer.clone(), network.clone());
		let _network_events = network.event_stream();

		// Can't send messages until identified.
		sm.send_message(remote_validator_id, hello.clone());

		// The observer should be notified of this.
		assert_eq!(
			observer.inner.borrow_mut().unidentified_node.pop(),
			Some(())
		);

		// Identify the local node.
		sm.identify(local_validator_id);
		assert_eq!(sm.local_validator_id, Some(local_validator_id));

		let (remote_sender, mut remote_receiver) = unbounded();
		network.add_remote_peer(remote_peer, remote_sender);

		// Simulate the remote peer identifying herself.
		sm.new_peer(&remote_peer);
		assert!(
			sm.peer_to_validator.contains_key(&remote_peer),
			"Entry should have been inserted."
		);
		sm.received(
			&remote_peer,
			vec![ProtocolMessage::Identify(remote_validator_id)],
		);
		assert_eq!(
			sm.peer_to_validator
				.get(&remote_peer)
				.expect("an entry for the remote peer")
				.expect("a validator id for the remote peer"),
			remote_validator_id
		);

		// The observer should be notified of this.
		assert_eq!(
			observer.inner.borrow_mut().new_peers.pop(),
			Some(remote_validator_id)
		);

		// The remote should have received an Identification reply.
		match remote_receiver
			.try_next()
			.expect("Should have received a message")
		{
			Some(Event::NotificationsReceived {
				remote,
				mut messages,
				..
			}) => {
				assert_eq!(remote, local_peer);
				if let Some((_, message)) = messages.pop() {
					assert_eq!(
						sm.try_decode(message.as_ref()).unwrap(),
						ProtocolMessage::Identify(local_validator_id)
					);
				} else {
					panic!("Expected a message.");
				}
			}
			_ => panic!("Expected an indentification message."),
		}

		// Simulate receiving a message.
		sm.received(&remote_peer, vec![ProtocolMessage::Message(hello.clone())]);

		// The observer should be notified of this with the peer's validator id.
		assert_eq!(
			observer.inner.borrow_mut().messages_received.pop(),
			Some((remote_validator_id, hello.clone()))
		);

		// Try to send an empty message.
		sm.send_message(remote_validator_id, RawMessage(vec![]));

		// The observer should be notified of this.
		assert_eq!(observer.inner.borrow_mut().empty_message.pop(), Some(()));

		// Try register under a new id.
		sm.identify(ValidatorId([0x44; 32]));
		assert_eq!(sm.local_validator_id, Some(local_validator_id));

		// The observer should be notified of this.
		assert_eq!(
			observer.inner.borrow_mut().already_identified.pop(),
			Some(local_validator_id)
		);

		// Try to send to an unregistered validator.
		let unregistered = ValidatorId([0xA1; 32]);
		sm.send_message(unregistered, hello);

		// The observer should be notified of this.
		assert_eq!(
			observer.inner.borrow_mut().unknown_recipients.pop(),
			Some(unregistered)
		);

		// Simulate a remote peer disconnect.
		sm.disconnected(&remote_peer);

		// The observer should be notified of this.
		assert_eq!(
			observer.inner.borrow_mut().disconnected_peers.pop(),
			Some(remote_validator_id)
		);
	}
}
