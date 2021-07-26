use anyhow::Result;
use core::iter;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{Future, Stream, StreamExt};
use log::debug;
use sc_network::{multiaddr, Event, ExHashT, NetworkService, PeerId};
use serde::{Serialize, Deserialize};
use sp_runtime::sp_std::sync::Arc;
use sp_runtime::traits::Block as BlockT;
use std::borrow::Cow;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValidatorId(
	pub [u8; 32]
);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMessage(
	pub Vec<u8>
);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ProtocolMessage {
	Identify(ValidatorId),
	Message(RawMessage),
}

pub const CHAINFLIP_P2P_PROTOCOL_NAME: &str = "/chainflip-protocol";

pub fn p2p_peers_set_config() -> sc_network::config::NonDefaultSetConfig {
	sc_network::config::NonDefaultSetConfig {
		notifications_protocol: CHAINFLIP_P2P_PROTOCOL_NAME.into(),
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

trait PeerNetwork: Clone {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn add_set_reserved(&self, who: PeerId, protocol: Cow<'static, str>);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_set_reserved(&self, who: PeerId, protocol: Cow<'static, str>);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, protocol: Cow<'static, str>, message: ProtocolMessage);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
}

impl<B: BlockT, H: ExHashT> PeerNetwork for Arc<NetworkService<B, H>> {
	fn add_set_reserved(&self, who: PeerId, protocol: Cow<'static, str>) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result =
			NetworkService::add_peers_to_reserved_set(self, protocol, iter::once(addr).collect());
		if let Err(err) = result {
			log::error!(target: "p2p", "add_set_reserved failed: {}", err);
		}
	}

	fn remove_set_reserved(&self, who: PeerId, protocol: Cow<'static, str>) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result = NetworkService::remove_peers_from_reserved_set(
			self,
			protocol,
			iter::once(addr).collect(),
		);
		if let Err(err) = result {
			log::error!(target: "p2p", "remove_set_reserved failed: {}", err);
		}
	}

	fn write_notification(&self, target: PeerId, protocol: Cow<'static, str>, message: ProtocolMessage) {
		let _ = bincode::serialize(&message).map(|bytes| {
			NetworkService::write_notification(self, target, protocol, bytes);
		}).unwrap_or_else(|err| {
			log::error!("Error while serializing p2p protocol message {}", err);
		});
	}

	fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
		Box::pin(NetworkService::event_stream(self, "network-chainflip"))
	}
}

/// An external observer for events. 
pub trait NetworkObserver {
	/// On a new peer connected to the network
	fn new_peer(&self, validator_id: &ValidatorId);
	/// On a peer being disconnected
	fn disconnected(&self, validator_id: &ValidatorId);
	/// A message being received from validator_id for this peer
	fn received(&self, validator_id: &ValidatorId, message: RawMessage);
}

/// A state machine routing messages and events to our network and observer
struct StateMachine<Observer: NetworkObserver, Network: PeerNetwork> {
	/// A reference to an NetworkObserver
	observer: Arc<Observer>,
	/// The peer to peer network
	network: Network,
	/// PeerIds with the corresponding ValidatorId, if available.
	peer_to_validator: HashMap<PeerId, Option<ValidatorId>>,
	/// ValidatorIds mapped to corresponding PeerIds.
	validator_to_peer: HashMap<ValidatorId, PeerId>,
	/// The protocol's name
	protocol: Cow<'static, str>,
}

const EXPECTED_PEER_COUNT: usize = 300;

impl<Observer, Network> StateMachine<Observer, Network>
where
	Observer: NetworkObserver,
	Network: PeerNetwork,
{
	pub fn new(observer: Arc<Observer>, network: Network, protocol: Cow<'static, str>) -> Self {
		StateMachine {
			observer,
			network,
			peer_to_validator: HashMap::with_capacity(EXPECTED_PEER_COUNT),
			validator_to_peer: HashMap::with_capacity(EXPECTED_PEER_COUNT),
			protocol,
		}
	}

	/// A new peer has arrived, insert into our internal list and await identification.
	pub fn register_peer(&mut self, peer_id: &PeerId) {
		self.peer_to_validator.insert(peer_id.clone(), None);
	}

	/// A peer has identified itself. Register the validator Id and notify the observer.
	pub fn register_validator_id(&mut self, peer_id: &PeerId, validator_id: ValidatorId) {
		self.peer_to_validator.entry(peer_id.clone()).or_insert(Some(validator_id));
		self.validator_to_peer.insert(validator_id, peer_id.clone());
		self.observer.new_peer(&validator_id);
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
		}
	}

	/// Messages received from peer_id, notify observer.
	pub fn received(&mut self, peer_id: &PeerId, messages: Vec<ProtocolMessage>) {
		if !self.peer_to_validator.contains_key(peer_id) {
			log::error!("Received message from unrecognised peer {:?}", peer_id);
			return;
		}

		for message in messages {
			match message {
				ProtocolMessage::Identify(validator_id) => {
					self.register_validator_id(peer_id, validator_id);
				},
				ProtocolMessage::Message(raw_message) => {
					self.maybe_notify_observer(peer_id, raw_message);
				},
			}
		}
	}

	/// Identify ourselves to the network.
	pub fn identify(&self, validator_id: ValidatorId) {
		for peer_id in self.peer_to_validator.keys() {
			self.network
				.write_notification(*peer_id, self.protocol.clone(), ProtocolMessage::Identify(validator_id));
		}
	}

	/// Send message to peer, this will fail silently if peer isn't in our peer list or if the message
	/// is empty.
	pub fn send_message(&self, validator_id: ValidatorId, message: RawMessage) {
		if message.0.is_empty() {
			return;
		}

		if let Some(peer_id) = self.validator_to_peer.get(&validator_id) {
			self.network
				.write_notification(*peer_id, self.protocol.clone(), ProtocolMessage::Message(message));
		}
	}

	/// Broadcast message to a specific list of peers on the network, this will fail silently if the message is empty.
	pub fn broadcast(&self, validators: Vec<ValidatorId>, message: RawMessage) {
		if message.0.is_empty() {
			return;
		}
		for validator_id in validators {
			self.send_message(validator_id, message.clone());
		}
	}

	/// Broadcast message to all known validators on the network, this will fail silently if the message is empty.
	pub fn broadcast_all(&self, message: RawMessage) {
		if !message.0.is_empty() {
			for peer_id in self.validator_to_peer.values() {
				self.network
					.write_notification(*peer_id, self.protocol.clone(), ProtocolMessage::Message(message.clone()));
			}
		}
	}
}

/// The entry point.  The network bridge provides the trait `Messaging`.
struct NetworkBridge<Observer: NetworkObserver, Network: PeerNetwork> {
	state_machine: StateMachine<Observer, Network>,
	network_event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
	protocol: Cow<'static, str>,
	worker: UnboundedReceiver<MessagingCommand>,
}

impl<Observer, Network> NetworkBridge<Observer, Network>
where
	Observer: NetworkObserver,
	Network: PeerNetwork,
{
	pub fn new(
		observer: Arc<Observer>,
		network: Network,
		protocol: Cow<'static, str>,
	) -> (Self, Arc<Mutex<Sender>>) {
		let state_machine = StateMachine::new(observer, network.clone(), protocol.clone());
		let network_event_stream = Box::pin(network.event_stream());
		let (sender, worker) = unbounded::<MessagingCommand>();
		let messenger = Arc::new(Mutex::new(Sender(sender.clone())));
		(
			NetworkBridge {
				state_machine,
				network_event_stream,
				protocol: protocol.clone(),
				worker,
			},
			messenger.clone(),
		)
	}
}

pub enum MessagingCommand {
	Identify(ValidatorId),
	Send(ValidatorId, RawMessage),
	Broadcast(Vec<ValidatorId>, RawMessage),
	BroadcastAll(RawMessage),
}

/// Messaging by sending directly or broadcasting
pub trait P2pMessaging {
	fn identify(&mut self, validator_id: ValidatorId) -> Result<()>;
	fn send_message(&mut self, validator_id: ValidatorId, data: RawMessage) -> Result<()>;
	fn broadcast(&self, validators: Vec<ValidatorId>, data: RawMessage) -> Result<()>;
	fn broadcast_all(&self, data: RawMessage) -> Result<()> {
		self.broadcast(vec![], data)
	}
}

/// Push messages down our channel to be passed on to the network
pub struct Sender(UnboundedSender<MessagingCommand>);
impl P2pMessaging for Sender {
	fn identify(&mut self, validator_id: ValidatorId) -> Result<()> {
		self.0.unbounded_send(MessagingCommand::Identify(validator_id))?;
		Ok(())
	}

	fn send_message(&mut self, validator_id: ValidatorId, data: RawMessage) -> Result<()> {
		self.0.unbounded_send(MessagingCommand::Send(validator_id, data))?;
		Ok(())
	}

	fn broadcast(&self, validators: Vec<ValidatorId>, data: RawMessage) -> Result<()> {
		self.0.unbounded_send(MessagingCommand::Broadcast(validators, data))?;
		Ok(())
	}
}

impl<Observer, Network> Unpin for NetworkBridge<Observer, Network>
where
	Observer: NetworkObserver,
	Network: PeerNetwork,
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
			match this.worker.poll_next_unpin(cx) {
				Poll::Ready(Some(cmd)) => {
					match cmd {
						MessagingCommand::Send(validator_id, msg) => {
							this.state_machine.send_message(validator_id, msg);
						},
						MessagingCommand::Broadcast(validators, msg) => {
							this.state_machine.broadcast(validators, msg);
						},
        				MessagingCommand::BroadcastAll(msg) => {
							this.state_machine.broadcast_all(msg);
						},
        				MessagingCommand::Identify(validator_id) => {
							this.state_machine.identify(validator_id);
						},
					}
				}
				Poll::Ready(None) => return Poll::Ready(()),
				Poll::Pending => break,
			}
		}

		loop {
			match this.network_event_stream.poll_next_unpin(cx) {
				Poll::Ready(Some(event)) => match event {
					Event::SyncConnected { remote } => {
						this.state_machine
							.network
							.add_set_reserved(remote, this.protocol.clone());
					}
					Event::SyncDisconnected { remote } => {
						this.state_machine
							.network
							.remove_set_reserved(remote, this.protocol.clone());
					}
					Event::NotificationStreamOpened {
						remote,
						protocol,
						role: _,
					} => {
						if protocol != this.protocol {
							continue;
						}
						this.state_machine.register_peer(&remote);
					}
					Event::NotificationStreamClosed { remote, protocol } => {
						if protocol != this.protocol {
							continue;
						}
						this.state_machine.disconnected(&remote);
					}
					Event::NotificationsReceived { remote, messages } => {
						if !messages.is_empty() {
							let messages: Vec<ProtocolMessage> = messages
								.into_iter()
								.filter_map(|(engine, data)| {
									if engine == this.protocol {
										bincode::deserialize(data.as_ref()).map_err(|err| {
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
				},
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
	use futures::Stream;
	use futures::{
		channel::mpsc::{unbounded, UnboundedSender},
		executor::block_on,
		future::poll_fn,
		FutureExt,
	};
	use sc_network::{Event, ObservedRole, PeerId};
	use std::sync::{Arc, Mutex};

	#[derive(Clone, Default)]
	struct TestNetwork {
		inner: Arc<Mutex<TestNetworkInner>>,
	}

	#[derive(Clone, Default)]
	struct TestNetworkInner {
		event_senders: Vec<UnboundedSender<Event>>,
		notifications: Vec<(Vec<u8>, Vec<u8>)>,
	}

	impl PeerNetwork for TestNetwork {
		fn add_set_reserved(&self, _who: PeerId, _protocol: Cow<'static, str>) {}

		fn remove_set_reserved(&self, _who: PeerId, _protocol: Cow<'static, str>) {}

		fn write_notification(&self, who: PeerId, _protocol: Cow<'static, str>, message: Vec<u8>) {
			self.inner
				.lock()
				.unwrap()
				.notifications
				.push((who.to_bytes(), message));
		}

		fn event_stream(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
			let (tx, rx) = unbounded();
			self.inner.lock().unwrap().event_senders.push(tx);

			Box::pin(rx)
		}
	}

	#[derive(Clone, Default)]
	struct TestObserver {
		inner: Arc<Mutex<TestObserverInner>>,
	}

	#[derive(Clone, Default)]
	struct TestObserverInner(Option<Vec<u8>>, Option<Vec<u8>>, Option<(Vec<u8>, Vec<u8>)>);

	impl NetworkObserver for TestObserver {
		fn new_peer(&self, peer_id: &PeerId) {
			self.inner.lock().unwrap().0 = Some(peer_id.to_bytes());
		}

		fn disconnected(&self, peer_id: &PeerId) {
			self.inner.lock().unwrap().1 = Some(peer_id.to_bytes());
		}

		fn received(&self, peer_id: &PeerId, messages: Message) {
			self.inner.lock().unwrap().2 = Some((peer_id.to_bytes(), messages));
		}
	}

	#[test]
	fn send_message_to_peer() {
		let network = TestNetwork::default();
		let protocol = Cow::Borrowed("/chainflip-protocol");
		let observer = Arc::new(TestObserver::default());
		let (mut bridge, communications) =
			NetworkBridge::new(observer.clone(), network.clone(), protocol.clone());

		let peer = PeerId::random();

		// Register peer
		let mut event_sender = network.inner.lock().unwrap().event_senders.pop().unwrap();

		let msg = Event::NotificationStreamOpened {
			remote: peer.clone(),
			protocol: protocol.clone(),
			role: ObservedRole::Authority,
		};

		event_sender
			.start_send(msg)
			.expect("Event stream is unbounded");

		block_on(poll_fn(|cx| {
			let mut sent = false;
			loop {
				if let Poll::Ready(()) = bridge.poll_unpin(cx) {
					unreachable!("we should have a new network event");
				}

				let o = observer.inner.lock().unwrap();

				if let Some(_) = &o.0 {
					if !sent {
						communications
							.lock()
							.unwrap()
							.send_message(&peer, b"this rocks".to_vec());
						sent = true;
					}

					if let Some(notification) =
						network.inner.lock().unwrap().notifications.pop().as_ref()
					{
						assert_eq!(
							notification.clone(),
							(peer.to_bytes(), b"this rocks".to_vec())
						);
						break;
					}
				}
			}
			Poll::Ready(())
		}));
	}

	#[test]
	fn broadcast_message_to_peers() {
		let network = TestNetwork::default();
		let protocol = Cow::Borrowed("/chainflip-protocol");
		let observer = Arc::new(TestObserver::default());
		let (mut bridge, comms) =
			NetworkBridge::new(observer.clone(), network.clone(), protocol.clone());

		let peer = PeerId::random();
		let peer_1 = PeerId::random();

		// Register peers
		let mut event_sender = network.inner.lock().unwrap().event_senders.pop().unwrap();

		let msg = Event::NotificationStreamOpened {
			remote: peer.clone(),
			protocol: protocol.clone(),
			role: ObservedRole::Authority,
		};
		event_sender
			.start_send(msg)
			.expect("Event stream is unbounded");

		let msg = Event::NotificationStreamOpened {
			remote: peer_1.clone(),
			protocol: protocol.clone(),
			role: ObservedRole::Authority,
		};

		event_sender
			.start_send(msg)
			.expect("Event stream is unbounded");

		block_on(poll_fn(|cx| {
			let mut sent = false;
			loop {
				if let Poll::Ready(()) = bridge.poll_unpin(cx) {
					unreachable!("we should have a new network event");
				}

				let o = observer.inner.lock().unwrap();

				if sent {
					let notifications: &Vec<(Vec<u8>, Vec<u8>)> =
						&network.inner.lock().unwrap().notifications;
					let peer_ids: Vec<PeerId> = notifications
						.into_iter()
						.map(|(id, _)| PeerId::from_bytes(id).unwrap())
						.collect();
					assert_eq!(peer_ids.len(), 2);
					assert!(peer_ids.contains(&peer));
					assert!(peer_ids.contains(&peer_1));
					assert_eq!(notifications[0].1, b"this rocks".to_vec());
					assert_eq!(notifications[1].1, b"this rocks".to_vec());
					break;
				}

				if let Some(_) = &o.0 {
					if !sent {
						comms.lock().unwrap().broadcast(b"this rocks".to_vec());
						sent = true;
					}
				}
			}
			Poll::Ready(())
		}));
	}
}
