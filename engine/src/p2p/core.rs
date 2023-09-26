mod auth;
mod monitor;
mod socket;
#[cfg(test)]
mod tests;

use std::{
	collections::{BTreeMap, HashMap},
	future::Future,
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
		let ed_public_key = ed25519_dalek::PublicKey::from_bytes(&ed_public_key.0).unwrap();
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

/// Used to track "registration" status on the network
enum RegistrationStatus {
	/// The node is not yet known to the network (its peer info
	/// may not be known to the network yet)
	Pending,
	/// The node is registered, i.e. its peer info has been
	/// recorded/updated
	Registered,
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
	ed25519_pk: &ed25519_dalek::PublicKey,
) -> x25519_dalek::PublicKey {
	use curve25519_dalek::edwards::CompressedEdwardsY;
	let ed_point = CompressedEdwardsY::from_slice(&ed25519_pk.to_bytes()).decompress().unwrap();
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

	fn reset(&mut self, account_id: &AccountId) {
		if self.reconnect_delays.remove(account_id).is_some() {
			tracing::debug!("Reconnection delay for {} is reset", account_id);
		}
		P2P_RECONNECT_PEERS.set(self.reconnect_delays.len());
	}
}

struct ActiveConnectionWrapper {
	metric: &'static P2P_ACTIVE_CONNECTIONS,
	map: BTreeMap<AccountId, ConnectedOutgoingSocket>,
}

impl ActiveConnectionWrapper {
	fn new() -> ActiveConnectionWrapper {
		ActiveConnectionWrapper { metric: &P2P_ACTIVE_CONNECTIONS, map: Default::default() }
	}
	fn get(&self, account_id: &AccountId) -> Option<&ConnectedOutgoingSocket> {
		self.map.get(account_id)
	}
	fn insert(
		&mut self,
		key: AccountId,
		value: ConnectedOutgoingSocket,
	) -> Option<ConnectedOutgoingSocket> {
		let result = self.map.insert(key, value);
		self.metric.set(self.map.len());
		result
	}
	fn remove(&mut self, key: &AccountId) -> Option<ConnectedOutgoingSocket> {
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
	/// Contains all existing ZMQ sockets for "client" connections. Note that ZMQ socket
	/// exists even when there is no internal TCP connection (e.g. before the connection
	/// is established for the first time, or when ZMQ it is reconnecting). Also, when
	/// our own (independent from ZMQ) reconnection mechanism kicks in, the entry is removed
	/// (because we don't want ZMQ's socket behaviour).
	/// NOTE: The mapping is from AccountId because we want to optimise for message
	/// sending, which uses AccountId
	active_connections: ActiveConnectionWrapper,
	/// NOTE: this is used for incoming messages when we want to map them to account_id
	/// NOTE: we don't use BTreeMap here because XPublicKey doesn't implement Ord.
	x25519_to_account_id: HashMap<XPublicKey, AccountId>,
	peer_infos: BTreeMap<AccountId, PeerInfo>,
	/// Channel through which we send incoming messages to the multisig
	incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	own_peer_info_sender: UnboundedSender<PeerInfo>,
	reconnect_context: ReconnectContext,
	/// This is how we communicate with the "monitor" thread
	monitor_handle: monitor::MonitorHandle,
	/// Our own "registration" status on the network
	status: RegistrationStatus,
	our_account_id: AccountId,
	/// NOTE: zmq context is intentionally declared at the bottom of the struct
	/// to ensure its destructor is called after that of any zmq sockets
	zmq_context: zmq::Context,
}

pub(super) fn start(
	p2p_key: &P2PKey,
	port: Port,
	current_peers: Vec<PeerInfo>,
	our_account_id: AccountId,
) -> (
	UnboundedSender<OutgoingMultisigStageMessages>,
	UnboundedSender<PeerUpdate>,
	UnboundedReceiver<(AccountId, Vec<u8>)>,
	UnboundedReceiver<PeerInfo>,
	impl Future<Output = ()>,
) {
	debug!("Our derived x25519 pubkey: {}", pk_to_string(&p2p_key.encryption_key.public_key));

	let zmq_context = zmq::Context::new();

	zmq_context.set_max_sockets(65536).expect("should update socket limit");

	// TODO: consider keeping track of "last activity" on any outgoing
	// socket connection and disconnecting inactive peers (see proxy_expire_idle_peers
	// in OxenMQ)

	let authenticator = auth::start_authentication_thread(zmq_context.clone());

	let (incoming_message_sender, incoming_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (reconnect_sender, reconnect_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (monitor_handle, monitor_event_receiver) =
		monitor::start_monitoring_thread(zmq_context.clone());

	// A channel used to notify whenever our own peer info changes on SC
	let (own_peer_info_sender, own_peer_info_receiver) = tokio::sync::mpsc::unbounded_channel();

	let mut context = P2PContext {
		zmq_context,
		key: p2p_key.encryption_key.clone(),
		monitor_handle,
		authenticator,
		active_connections: ActiveConnectionWrapper::new(),
		x25519_to_account_id: Default::default(),
		peer_infos: Default::default(),
		reconnect_context: ReconnectContext::new(reconnect_sender),
		incoming_message_sender,
		own_peer_info_sender,
		our_account_id,
		status: RegistrationStatus::Pending,
	};

	debug!("Registering peer info for {} peers", current_peers.len());
	for peer_info in current_peers {
		context.handle_peer_update(peer_info);
	}

	let incoming_message_receiver_ed25519 = context.start_listening_thread(port);

	let (out_msg_sender, out_msg_receiver) = tokio::sync::mpsc::unbounded_channel();
	let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

	let fut = context
		.control_loop(
			out_msg_receiver,
			incoming_message_receiver_ed25519,
			peer_update_receiver,
			monitor_event_receiver,
			reconnect_receiver,
		)
		.instrument(info_span!("p2p"));

	(out_msg_sender, peer_update_sender, incoming_message_receiver, own_peer_info_receiver, fut)
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
			}
		}
	}

	fn send_messages(&self, messages: OutgoingMultisigStageMessages) {
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

	fn send_message(&self, account_id: AccountId, payload: Vec<u8>) {
		match self.active_connections.get(&account_id) {
			Some(socket) => {
				socket.send(payload);
				P2P_MSG_SENT.inc();
			},
			None => {
				warn!("Failed to send message. Peer not registered: {account_id}")
			},
		}
	}

	fn on_peer_update(&mut self, update: PeerUpdate) {
		match update {
			PeerUpdate::Registered(peer_info) => self.handle_peer_update(peer_info),
			PeerUpdate::Deregistered(account_id, _pubkey) => self.remove_peer(account_id),
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

	fn remove_peer_and_disconnect_socket(&mut self, socket: ConnectedOutgoingSocket) {
		let pubkey = &socket.peer().pubkey;
		self.authenticator.remove_peer(pubkey);
		assert!(
			self.x25519_to_account_id.remove(pubkey).is_some(),
			"Invariant violation: pubkey must be present"
		);
	}

	/// Removing a peer means: (1) removing it from the list of allowed nodes,
	/// (2) disconnecting our "client" socket with that node, (3) removing
	/// any references to it in local state (mappings)
	fn remove_peer(&mut self, account_id: AccountId) {
		// NOTE: There is no (trivial) way to disconnect peers that are
		// already connected to our listening ZMQ socket, we can only
		// prevent future connections from being established and rely
		// on peer from disconnecting from "client side".
		// TODO: ensure that stale/inactive connections are terminated

		if let Some(existing_socket) = self.active_connections.remove(&account_id) {
			self.remove_peer_and_disconnect_socket(existing_socket);
		} else {
			error!("Failed remove unknown peer: {account_id}");
		}

		if self.peer_infos.remove(&account_id).is_none() {
			error!("Failed to remove peer info for unknown peer: {account_id}");
		}

		// There may or may not be a reconnection delay for
		// this node, but we reset it just in case:
		self.reconnect_context.reset(&account_id);
	}

	/// Reconnect to peer assuming that its peer info hasn't changed
	fn handle_monitor_event(&mut self, event: MonitorEvent) {
		match event {
			MonitorEvent::ConnectionFailure(account_id) => {
				let Some(_existing_socket) = self.active_connections.remove(&account_id) else {
					// NOTE: this should not happen, but this guards against any surprising ZMQ
					// behaviour
					error!("Unexpected attempt to reconnect to an existing peer: {account_id}");
					return
				};

				self.reconnect_context.schedule_reconnect(account_id);
			},
			MonitorEvent::ConnectionSuccess(account_id) => {
				self.reconnect_context.reset(&account_id);
			},
		};
	}

	fn reconnect_to_peer(&mut self, account_id: &AccountId) {
		if let Some(peer_info) = self.peer_infos.get(account_id) {
			info!("Reconnecting to peer: {}", peer_info.account_id);

			// It is possible that while we were waiting to reconnect,
			// we received a peer info update and created a new "connection".
			// This connection might be "healthy", but it is safer/easier to
			// remove it and proceed with reconnecting.
			if self.active_connections.remove(account_id).is_some() {
				debug!("Reconnecting to a peer that's already connected: {}. Existing connection was removed.", account_id);
			}
			self.connect_to_peer(peer_info.clone());
		} else {
			error!("Failed to reconnect to peer {account_id}. (Peer info not found.)");
		}
	}

	fn connect_to_peer(&mut self, peer: PeerInfo) {
		let account_id = peer.account_id.clone();

		let socket = OutgoingSocket::new(&self.zmq_context, &self.key);

		self.monitor_handle.start_monitoring_for(&socket, &peer);

		let connected_socket = socket.connect(peer);

		if let Some(old_socket) = self.active_connections.insert(account_id, connected_socket) {
			// This should not happen because we always remove existing connection/socket
			// prior to connecting, but even if it does, it should be OK to replace the
			// connection (this doesn't break any invariants and the new peer info is
			// likely to be more up-to-date).
			warn!("Replacing existing ZMQ socket: {:?}", old_socket.peer());
		}
	}

	fn handle_own_registration(&mut self, own_info: PeerInfo) {
		debug!("Received own node's registration. Starting to connect to peers.");

		self.own_peer_info_sender.send(own_info).unwrap();

		if let RegistrationStatus::Pending = &mut self.status {
			let peers: Vec<_> = self.peer_infos.values().cloned().collect();
			// Connect to all outstanding peers
			for peer in peers {
				self.connect_to_peer(peer)
			}
			self.status = RegistrationStatus::Registered;
		};
	}

	fn add_or_update_peer(&mut self, peer: PeerInfo) {
		if let Some(existing_socket) = self.active_connections.remove(&peer.account_id) {
			debug!(
				peer_info = peer.to_string(),
				"Received info for known peer with account id {}, updating info and reconnecting",
				&peer.account_id
			);

			self.remove_peer_and_disconnect_socket(existing_socket);
		} else {
			debug!(
				peer_info = peer.to_string(),
				"Received info for new peer with account id {}, adding to allowed peers and id mapping",
				&peer.account_id
			);
		}

		self.authenticator.add_peer(&peer);

		self.peer_infos.insert(peer.account_id.clone(), peer.clone());

		self.x25519_to_account_id.insert(peer.pubkey, peer.account_id.clone());

		match &mut self.status {
			RegistrationStatus::Pending => {
				// We will connect to all peers in `self.peer_infos` once we receive our own
				// registration
				info!("Delaying connecting to {}", peer.account_id);
			},
			RegistrationStatus::Registered => {
				self.connect_to_peer(peer);
			},
		}
	}

	fn handle_peer_update(&mut self, peer: PeerInfo) {
		if peer.account_id == self.our_account_id {
			self.handle_own_registration(peer);
		} else {
			self.add_or_update_peer(peer);
		}
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
