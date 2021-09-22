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
use futures::stream::Fuse;
use core::iter;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{Stream, StreamExt};
use sc_network::{multiaddr, Event, ExHashT, NetworkService, PeerId};
use serde::{Deserialize, Serialize};
use sp_runtime::sp_std::sync::Arc;
use sp_runtime::traits::Block as BlockT;
use std::borrow::Cow;
use std::collections::HashMap;
use std::pin::Pin;

// TODO: This is duplicated in the CFE, can we just use one of these?
/// The type of validator id expected by the p2p layer, uses standard serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub [u8; 32]);

/// A wrapper around a byte buffer containing some opaque message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMessage(pub Vec<u8>);

/// The protocol has two message types, `Identify` and `Message`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ProtocolMessage {
	Identify(AccountId),
	Message(RawMessage),
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
	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
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

	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
		Box::pin(self.event_stream("network-chainflip"))
	}
}

/// A collection of callbacks for network events.
pub trait NetworkObserver {
	/// Called when a peer identifies itself to the network.
	fn new_validator(&self, validator_id: &AccountId);
	/// Called when a peer is disconnected.
	fn disconnected(&self, validator_id: &AccountId);
	/// Called when a message is received from some validator_id for this peer.
	fn received(&self, from: &AccountId, message: RawMessage);
	/// Called when a message could not be delivered because the recipient is unknown.
	fn unknown_recipient(&self, recipient_id: &AccountId);
	/// Called when a message is sent before identifying the node to the network.
	fn unidentified_node(&self);
	/// Empty messages are not allowed.
	fn empty_message(&self);
	/// A node cannot identify more than once.
	fn already_identified(&self, existing_id: &AccountId);
}

/// Defines the logic for processing network events and commands from this node.
///
/// ## ID management
///
/// Peers must identify themselves by their `AccountId` otherwise they will be unable to send
/// messages.
/// Likewise, any messages received from peers that have not identified themselves will be dropped.
struct StateMachine<Observer: NetworkObserver, Network: PeerNetwork> {
	/// A reference to a NetworkObserver
	observer: Arc<Observer>,
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
	fn register_identification(&mut self, peer_id: &PeerId, validator_id: AccountId) {
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
	pub fn identify(&mut self, validator_id: AccountId) {
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
	fn send_identification(&self, peer_id: PeerId, validator_id: AccountId) {
		self.encode_and_send(peer_id, ProtocolMessage::Identify(validator_id));
	}

	/// Send message to peer, this will fail silently if peer isn't in our peer list or if the message
	/// is empty.
	pub fn send_message(&self, validator_id: AccountId, message: RawMessage) {
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
	network_event_stream: Fuse<Pin<Box<dyn Stream<Item = Event> + Send>>>,
	command_receiver: UnboundedReceiver<MessagingCommand>,
}

impl<Observer: NetworkObserver, Network: PeerNetwork> NetworkBridge<Observer, Network> {
	pub fn new(
		observer: Arc<Observer>,
		p2p_network: Arc<Network>,
	) -> (Self, Arc<UnboundedSender<MessagingCommand>>) {
		let state_machine = StateMachine::new(observer, p2p_network.clone());
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
	Identify(AccountId),
	Send(AccountId, RawMessage),
	Broadcast(Vec<AccountId>, RawMessage),
	BroadcastAll(RawMessage),
}

impl<O, N> NetworkBridge<O, N>
where
	O: NetworkObserver,
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
								MessagingCommand::Identify(validator_id) => {
									self.state_machine.identify(validator_id);
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
										let messages: Vec<ProtocolMessage> =
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
