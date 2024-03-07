mod auth;
mod monitor;
mod socket;
#[cfg(test)]
mod tests;

use std::{
	cell::Cell,
	collections::{BTreeMap, HashMap},
	net::Ipv6Addr,
	sync::Arc,
	time::Duration,
};

use auth::Authenticator;
use serde::{Deserialize, Serialize};
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, error, info, info_span, trace, warn, Instrument};
use utilities::{
	make_periodic_tick,
	metrics::{
		P2P_ACTIVE_CONNECTIONS, P2P_BAD_MSG, P2P_MSG_RECEIVED, P2P_MSG_SENT, P2P_RECONNECT_PEERS,
	},
	Port,
};
use x25519_dalek::StaticSecret;

use crate::p2p::{pk_to_string, OutgoingMultisigStageMessages};
use monitor::MonitorEvent;

use socket::{ConnectedOutgoingSocket, OutgoingSocket, RECONNECT_INTERVAL, RECONNECT_INTERVAL_MAX};

use super::{EdPublicKey, P2PKey, XPublicKey};

/// How long to keep the TCP connection open for while waiting
/// for the client to authenticate themselves. We want to keep
/// this somewhat short to mitigate some attacks where clients
/// can use system resources without authenticating.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait until some activity on a socket (defined by a need to
/// send a message) before deeming the connection "stale" (the state in which
/// we drop the socket and are not actively trying to reconnect)
pub const MAX_INACTIVITY_THRESHOLD: Duration = Duration::from_secs(60 * 60);
/// How often to check for "stale" connections
pub const ACTIVITY_CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct X25519KeyPair {
	pub public_key: XPublicKey,
	pub secret_key: StaticSecret,
}

#[derive(Debug)]
pub enum PeerUpdate {
	Registered(PeerInfo),
	Deregistered(AccountId, EdPublicKey),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
	pub account_id: AccountId,
	pub pubkey: XPublicKey,
	pub ip: Ipv6Addr,
	pub port: Port,
}

impl PeerInfo {
	pub fn new(
		account_id: AccountId,
		ed_public_key: EdPublicKey,
		ip: Ipv6Addr,
		port: Port,
	) -> Self {
		let ed_public_key = ed25519_dalek::VerifyingKey::from_bytes(&ed_public_key.0).unwrap();
		let x_public_key = ed25519_public_key_to_x25519_public_key(&ed_public_key);

		PeerInfo { account_id, pubkey: x_public_key, ip, port }
	}

	pub fn zmq_endpoint(&self) -> String {
		format!("tcp://[{}]:{}", self.ip, self.port)
	}
}

impl std::fmt::Display for PeerInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(
			f,
			"PeerInfo {{ account_id: {}, pubkey: {}, ip: {}, port: {} }}",
			self.account_id,
			pk_to_string(&self.pubkey),
			self.ip,
			self.port,
		)
	}
}

pub fn ed25519_secret_key_to_x25519_secret_key(
	ed25519_sk: &ed25519_dalek::SecretKey,
) -> x25519_dalek::StaticSecret {
	use sha2::{Digest, Sha512};
	let mut h: Sha512 = Sha512::new();
	let mut hash: [u8; 64] = [0u8; 64];
	let mut digest: [u8; 32] = [0u8; 32];

	h.update(ed25519_sk);
	hash.copy_from_slice(h.finalize().as_slice());

	digest.copy_from_slice(&hash[..32]);
	x25519_dalek::StaticSecret::from(digest)
}

pub fn ed25519_public_key_to_x25519_public_key(
	ed25519_pk: &ed25519_dalek::VerifyingKey,
) -> x25519_dalek::PublicKey {
	use curve25519_dalek::edwards::CompressedEdwardsY;
	let ed_point = CompressedEdwardsY::from_slice(&ed25519_pk.to_bytes())
		.expect("VerifyingKey::to_bytes returns 32 bytes.")
		.decompress()
		.unwrap();
	let x_point = ed_point.to_montgomery();

	x25519_dalek::PublicKey::from(x_point.to_bytes())
}

struct ReconnectContext {
	reconnect_delays: BTreeMap<AccountId, std::time::Duration>,
	reconnect_sender: UnboundedSender<AccountId>,
}

impl ReconnectContext {
	fn new(reconnect_sender: UnboundedSender<AccountId>) -> Self {
		ReconnectContext { reconnect_delays: BTreeMap::new(), reconnect_sender }
	}

	fn get_delay_for(&mut self, account_id: &AccountId) -> std::time::Duration {
		use std::collections::btree_map::Entry;

		match self.reconnect_delays.entry(account_id.clone()) {
			Entry::Occupied(mut entry) => {
				let new_delay = std::cmp::min(*entry.get() * 2, RECONNECT_INTERVAL_MAX);
				*entry.get_mut() = new_delay;
				new_delay
			},
			Entry::Vacant(entry) => {
				let delay = RECONNECT_INTERVAL;
				entry.insert(delay);
				delay
			},
		}
	}

	fn schedule_reconnect(&mut self, account_id: AccountId) {
		let delay = self.get_delay_for(&account_id);

		tracing::debug!("Will reconnect to {} in {:?}", account_id, delay);
		P2P_RECONNECT_PEERS.set(self.reconnect_delays.len());
		tokio::spawn({
			let sender = self.reconnect_sender.clone();
			async move {
				tokio::time::sleep(delay).await;
				sender.send(account_id).unwrap();
			}
		});
	}

	// NOTE: we might already have a reconnection scheduled for
	// this (e.g. if we reset/cancel reconnection due to receiving
	// new peer info), but instead of trying to cancel it now, we
	// rely on the fact that we ignore any reconnection attempts
	// for peers not in `ReconnectionScheduled` state.
	fn reset(&mut self, account_id: &AccountId) {
		if self.reconnect_delays.remove(account_id).is_some() {
			tracing::debug!("Reconnection delay for {} is reset", account_id);
		}
		P2P_RECONNECT_PEERS.set(self.reconnect_delays.len());
	}
}

enum ConnectionState {
	// There is a ZMQ socket for this peer (which might or might
	// not be connected, but reconnection is handled by ZMQ).
	Connected(ConnectedOutgoingSocket),
	// There is no ZMQ socket for this peer (because we don't
	// want ZMQ's default behavior yet), but we have arranged
	// for a ZMQ socket to be created again in the future.
	ReconnectionScheduled,
	// There hasn't been recent interaction with the node, so we
	// don't maintain an active connection with it. We will connect
	// to it lazily if needed.
	Stale,
}

struct ConnectionStateInfo {
	state: ConnectionState,
	// Last time we received an instruction to send a message
	// to this node
	last_activity: Cell<tokio::time::Instant>,
	info: PeerInfo,
}

struct ActiveConnectionWrapper {
	metric: &'static P2P_ACTIVE_CONNECTIONS,
	map: BTreeMap<AccountId, ConnectionStateInfo>,
}

impl ActiveConnectionWrapper {
	fn new() -> ActiveConnectionWrapper {
		ActiveConnectionWrapper { metric: &P2P_ACTIVE_CONNECTIONS, map: Default::default() }
	}
	fn get(&self, account_id: &AccountId) -> Option<&ConnectionStateInfo> {
		self.map.get(account_id)
	}
	fn get_mut(&mut self, account_id: &AccountId) -> Option<&mut ConnectionStateInfo> {
		self.map.get_mut(account_id)
	}
	fn insert(
		&mut self,
		key: AccountId,
		value: ConnectionStateInfo,
	) -> Option<ConnectionStateInfo> {
		let result = self.map.insert(key, value);
		self.metric.set(self.map.len());
		result
	}
	fn remove(&mut self, key: &AccountId) -> Option<ConnectionStateInfo> {
		let result = self.map.remove(key);
		self.metric.set(self.map.len());
		result
	}
}

/// The state a nodes needs for p2p
struct P2PContext {
	/// Our own key, used for initiating and accepting secure connections
	key: X25519KeyPair,
	/// A handle to the authenticator thread that can be used to make changes to the
	/// list of allowed peers
	authenticator: Arc<Authenticator>,
	/// Contain entries for all nodes that we *should* be connected to (i.e. all registered
	/// nodes), which are either connected or scheduled for reconnection
	active_connections: ActiveConnectionWrapper,
	/// NOTE: this is used for incoming messages when we want to map them to account_id
	/// NOTE: we don't use BTreeMap here because XPublicKey doesn't implement Ord.
	x25519_to_account_id: HashMap<XPublicKey, AccountId>,
	/// Channel through which we send incoming messages to the multisig
	incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	reconnect_context: ReconnectContext,
	/// This is how we communicate with the "monitor" thread
	monitor_handle: monitor::MonitorHandle,
	our_account_id: AccountId,
	/// NOTE: zmq context is intentionally declared at the bottom of the struct
	/// to ensure its destructor is called after that of any zmq sockets
	zmq_context: zmq::Context,
}

pub(super) async fn start(
	p2p_key: P2PKey,
	port: Port,
	current_peers: Vec<PeerInfo>,
	our_account_id: AccountId,
	incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	outgoing_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
	peer_update_receiver: UnboundedReceiver<PeerUpdate>,
) {
	debug!("Our derived x25519 pubkey: {}", pk_to_string(&p2p_key.encryption_key.public_key));

	let zmq_context = zmq::Context::new();

	zmq_context.set_max_sockets(65536).expect("should update socket limit");

	let authenticator = auth::start_authentication_thread(zmq_context.clone());

	let (reconnect_sender, reconnect_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (monitor_handle, monitor_event_receiver) =
		monitor::start_monitoring_thread(zmq_context.clone());

	let mut context = P2PContext {
		zmq_context,
		key: p2p_key.encryption_key,
		monitor_handle,
		authenticator,
		active_connections: ActiveConnectionWrapper::new(),
		x25519_to_account_id: Default::default(),
		reconnect_context: ReconnectContext::new(reconnect_sender),
		incoming_message_sender,
		our_account_id,
	};

	debug!("Registering peer info for {} peers", current_peers.len());
	for peer_info in current_peers {
		context.add_or_update_peer(peer_info);
	}

	let incoming_message_receiver_ed25519 = context.start_listening_thread(port);

	context
		.control_loop(
			outgoing_message_receiver,
			incoming_message_receiver_ed25519,
			peer_update_receiver,
			monitor_event_receiver,
			reconnect_receiver,
		)
		.instrument(info_span!("p2p"))
		.await;
}

fn disconnect_socket(_socket: ConnectedOutgoingSocket) {
	// Simply dropping the socket is enough
}

impl P2PContext {
	async fn control_loop(
		mut self,
		mut outgoing_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
		mut incoming_message_receiver: UnboundedReceiver<(XPublicKey, Vec<u8>)>,
		mut peer_update_receiver: UnboundedReceiver<PeerUpdate>,
		mut monitor_event_receiver: UnboundedReceiver<MonitorEvent>,
		mut reconnect_receiver: UnboundedReceiver<AccountId>,
	) {
		let mut check_activity_interval = make_periodic_tick(ACTIVITY_CHECK_INTERVAL, false);

		loop {
			tokio::select! {
				Some(messages) = outgoing_message_receiver.recv() => {
					self.send_messages(messages);
				}
				Some(peer_update) = peer_update_receiver.recv() => {
					self.on_peer_update(peer_update);
				}
				Some((pubkey, payload)) = incoming_message_receiver.recv() => {
					// before we forward the messages to other modules we map
					// the x25519 pubkey to their account id here
					self.forward_incoming_message(pubkey, payload);
				}
				Some(event) = monitor_event_receiver.recv() => {
					self.handle_monitor_event(event);
				}
				Some(account_id) = reconnect_receiver.recv() => {
					self.reconnect_to_peer(&account_id);
				}
				_ = check_activity_interval.tick() => {
					self.check_activity();
				}
			}
		}
	}

	fn send_messages(&mut self, messages: OutgoingMultisigStageMessages) {
		match messages {
			OutgoingMultisigStageMessages::Broadcast(account_ids, payload) => {
				trace!("Broadcasting a message to all {} peers", account_ids.len());
				for acc_id in account_ids {
					self.send_message(acc_id, payload.clone());
				}
			},
			OutgoingMultisigStageMessages::Private(messages) => {
				trace!("Sending private messages to all {} peers", messages.len());
				for (acc_id, payload) in messages {
					self.send_message(acc_id, payload);
				}
			},
		}
	}

	fn send_message(&mut self, account_id: AccountId, payload: Vec<u8>) {
		if let Some(peer) = self.active_connections.get(&account_id) {
			peer.last_activity.set(tokio::time::Instant::now());

			match &peer.state {
				ConnectionState::Connected(socket) => {
					socket.send(payload);
					P2P_MSG_SENT.inc();
				},
				ConnectionState::ReconnectionScheduled => {
					// TODO: buffer the messages and send them later?
					warn!(
						"Failed to send message. Peer is scheduled for reconnection: {account_id}"
					);
				},
				ConnectionState::Stale => {
					// Connect and try again (there is no infinite loop here
					// since the state will be `Connected` after this)

					// This is guaranteed by construction of `active_connections`:
					assert_eq!(peer.info.account_id, account_id);

					self.connect_to_peer(peer.info.clone(), peer.last_activity.get());
					self.send_message(account_id, payload);
				},
			}
		} else {
			warn!("Failed to send message. Peer not registered: {account_id}")
		}
	}

	fn on_peer_update(&mut self, update: PeerUpdate) {
		match update {
			PeerUpdate::Registered(peer_info) => self.add_or_update_peer(peer_info),
			PeerUpdate::Deregistered(account_id, _pubkey) =>
				self.handle_peer_deregistration(account_id),
		}
	}

	fn forward_incoming_message(&mut self, pubkey: XPublicKey, payload: Vec<u8>) {
		if let Some(acc_id) = self.x25519_to_account_id.get(&pubkey) {
			trace!("Received a message from {acc_id}");
			self.incoming_message_sender.send((acc_id.clone(), payload)).unwrap();
		} else {
			P2P_BAD_MSG.inc(&["unknown_x25519_key"]);
			warn!("Received a message for an unknown x25519 key: {}", pk_to_string(&pubkey));
		}
	}

	fn clean_up_for_peer_pubkey(&mut self, pubkey: &XPublicKey) {
		self.authenticator.remove_peer(pubkey);
		if self.x25519_to_account_id.remove(pubkey).is_none() {
			error!("Invariant violation: pubkey must be present");
			debug_assert!(false, "Invariant violation: pubkey must be present");
		}
	}

	/// Removing a peer means: (1) removing it from the list of allowed nodes,
	/// (2) disconnecting our "client" socket with that node, (3) removing
	/// any references to it in local state (mappings)
	fn handle_peer_deregistration(&mut self, account_id: AccountId) {
		// NOTE: There is no (trivial) way to disconnect peers that are
		// already connected to our listening ZMQ socket, we can only
		// prevent future connections from being established and rely
		// on peer from disconnecting from "client side".

		if account_id == self.our_account_id {
			warn!("Received peer info deregistration of our own node!");
			return
		}

		if let Some(peer) = self.active_connections.remove(&account_id) {
			match peer.state {
				ConnectionState::Connected(existing_socket) => {
					disconnect_socket(existing_socket);
				},
				ConnectionState::ReconnectionScheduled => {
					self.reconnect_context.reset(&account_id);
				},
				ConnectionState::Stale => {
					// Nothing to do
				},
			}

			self.clean_up_for_peer_pubkey(&peer.info.pubkey);
		} else {
			error!("Failed remove unknown peer: {account_id}");
		}

		// There may or may not be a reconnection delay for
		// this node, but we reset it just in case:
		self.reconnect_context.reset(&account_id);
	}

	/// Reconnect to peer assuming that its peer info hasn't changed
	fn handle_monitor_event(&mut self, event: MonitorEvent) {
		match event {
			MonitorEvent::ConnectionFailure(account_id) => {
				self.reconnect_context.schedule_reconnect(account_id.clone());
				if let Some(peer) = self.active_connections.get_mut(&account_id) {
					peer.state = ConnectionState::ReconnectionScheduled;
				} else {
					error!("Unexpected attempt to reconnect to an unknown peer: {account_id}");
				}
			},
			MonitorEvent::ConnectionSuccess(account_id) => {
				self.reconnect_context.reset(&account_id);
			},
		};
	}

	fn reconnect_to_peer(&mut self, account_id: &AccountId) {
		if let Some(peer) = self.active_connections.remove(account_id) {
			match peer.state {
				ConnectionState::ReconnectionScheduled => {
					info!("Reconnecting to peer: {account_id}");
					self.connect_to_peer(peer.info.clone(), peer.last_activity.get());
				},
				ConnectionState::Connected(_) => {
					// It is possible that while we were waiting to reconnect,
					// we received a peer info update and created a new "connection".
					// It is safe to drop the reconnection attempt even if this
					// ZMQ connection is not "healthy" since reconnecting
					// is now in ZMQ's hands, and it shouldn't be possible that we
					// have missed any new `ConnectionFailure` event since we wouldn't
					// be in `Connected` state now.
					debug!(
						"Reconnection attempt to {} cancelled: ZMQ socket already exists.",
						account_id
					);
				},
				ConnectionState::Stale => {
					debug!(
						"Reconnection attempt to {} cancelled: connection is stale.",
						account_id
					);
				},
			}
		} else {
			debug!("Will not reconnect to now deregistered peer: {}", account_id);
		}
	}

	fn connect_to_peer(&mut self, peer: PeerInfo, previous_activity: tokio::time::Instant) {
		let account_id = peer.account_id.clone();

		let socket = OutgoingSocket::new(&self.zmq_context, &self.key);

		self.monitor_handle.start_monitoring_for(&socket, &peer);

		let connected_socket = socket.connect(peer.clone());

		if let Some(connection) = self.active_connections.insert(
			account_id.clone(),
			ConnectionStateInfo {
				state: ConnectionState::Connected(connected_socket),
				info: peer,
				last_activity: Cell::new(previous_activity),
			},
		) {
			if !matches!(connection.state, ConnectionState::Stale) {
				// This should not happen for non-stale sockets because we always remove
				// existing connection/socket prior to connecting, but even if it does,
				// it should be OK to replace the connection (this doesn't break any
				// invariants and the new peer info is likely to be more up-to-date).
				error!("Unexpected existing connection while connecting to {account_id}");
			}
		}
	}

	fn add_or_update_peer(&mut self, peer: PeerInfo) {
		if peer.account_id == self.our_account_id {
			// nothing to do
			return
		}

		let mut previous_activity = tokio::time::Instant::now();

		if let Some(existing_peer_state) = self.active_connections.remove(&peer.account_id) {
			debug!(
				peer_info = peer.to_string(),
				"Received info for known peer with account id {}, updating info and reconnecting",
				&peer.account_id
			);

			previous_activity = existing_peer_state.last_activity.get();

			match existing_peer_state.state {
				ConnectionState::Connected(socket) => {
					disconnect_socket(socket);
				},
				ConnectionState::ReconnectionScheduled => {
					self.reconnect_context.reset(&peer.account_id);
				},
				ConnectionState::Stale => {
					// nothing to do
				},
			}
			// Remove any state from previous peer info in case of update:
			self.clean_up_for_peer_pubkey(&existing_peer_state.info.pubkey);
		} else {
			debug!(
				peer_info = peer.to_string(),
				"Received info for new peer with account id {}, adding to allowed peers and id mapping",
				&peer.account_id
			);
		}

		self.authenticator.add_peer(&peer);

		self.x25519_to_account_id.insert(peer.pubkey, peer.account_id.clone());

		self.connect_to_peer(peer, previous_activity);
	}

	/// Start listening for incoming p2p messages on a separate thread
	fn start_listening_thread(&mut self, port: Port) -> UnboundedReceiver<(XPublicKey, Vec<u8>)> {
		let socket = self.zmq_context.socket(zmq::SocketType::ROUTER).unwrap();

		socket.set_router_mandatory(true).unwrap();
		socket.set_router_handover(true).unwrap();
		socket.set_curve_server(true).unwrap();
		socket.set_curve_secretkey(&self.key.secret_key.to_bytes()).unwrap();
		socket.set_handshake_ivl(HANDSHAKE_TIMEOUT.as_millis() as i32).unwrap();

		// Listen on all interfaces
		let endpoint = format!("tcp://0.0.0.0:{port}");
		info!("Started listening for incoming p2p connections on: {endpoint}");

		socket.bind(&endpoint).expect("invalid endpoint");

		let (incoming_message_sender, incoming_message_receiver) =
			tokio::sync::mpsc::unbounded_channel();

		// This OS thread is for incoming messages
		// TODO: combine this with the authentication thread?
		std::thread::spawn(move || loop {
			let mut parts = receive_multipart(&socket).unwrap();
			P2P_MSG_RECEIVED.inc();
			// We require that all messages exchanged between
			// peers only consist of one part. ZMQ dealer
			// sockets automatically prepend a sender id
			// (which we ignore) to every message, giving
			// us a 2 part message.
			if parts.len() == 2 {
				let msg = &mut parts[1];

				// This value is ZMQ convention for the public
				// key of message's origin
				const PUBLIC_KEY_TAG: &str = "User-Id";
				let pubkey = msg.gets(PUBLIC_KEY_TAG).expect("pubkey is always present");

				let pubkey: [u8; 32] = hex::decode(pubkey).unwrap().try_into().unwrap();
				let pubkey = XPublicKey::from(pubkey);

				incoming_message_sender.send((pubkey, msg.to_vec())).unwrap();
			} else {
				P2P_BAD_MSG.inc(&["bad_number_of_parts"]);
				warn!(
					"Ignoring a multipart message with unexpected number of parts ({})",
					parts.len()
				)
			}
		});

		incoming_message_receiver
	}

	fn check_activity(&mut self) {
		for (account_id, state) in &mut self.active_connections.map {
			if !matches!(state.state, ConnectionState::Stale) &&
				state.last_activity.get().elapsed() > MAX_INACTIVITY_THRESHOLD
			{
				debug!("Peer connection is deemed stale due to inactivity: {}", account_id);
				self.reconnect_context.reset(account_id);
				// ZMQ socket is dropped here
				state.state = ConnectionState::Stale;
			}
		}
	}
}

/// Unlike recv_multipart available on zmq::Socket, this collects
/// original message structs rather than payload bytes only
fn receive_multipart(socket: &zmq::Socket) -> zmq::Result<Vec<zmq::Message>> {
	// This indicates that we always want to block while
	// waiting for new messages
	let flags = 0;

	let mut parts = vec![];

	loop {
		let mut part = zmq::Message::new();
		socket.recv(&mut part, flags)?;
		parts.push(part);

		let more_parts = socket.get_rcvmore()?;
		if !more_parts {
			break
		}
	}
	Ok(parts)
}
