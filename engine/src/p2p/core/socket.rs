use std::time::Duration;

use super::{KeyPair, PeerInfo};

/// Wait this long until attempting to reconnect
const RECONNECT_INTERVAL: Duration = Duration::from_millis(250);
/// Reconnection uses exponential backoff: each reconnection attempt
/// waits for twice as long as the previous attempt, up to this maximum
const RECONNECT_INTERVAL_MAX: Duration = Duration::from_secs(5);

/// Maximum incoming message size: if a remote tries sending a message larger than
/// this they get disconnected (TODO: make sure this is slightly more that the
/// theoretical maximum needed for multisig; 2MB is a conservative estimate.)
const MAX_MESSAGE_SIZE: i64 = 2 * 1024 * 1024;

/// How often should ZMQ send heartbeat messages in order to detect
/// dead connections sooner (setting this to 0 disables heartbeats)
const CONNECTION_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
/// How long to wait for a heartbeat response before timing out the
/// connection
const CONNECTION_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);
/// An argument to set_linger on a socket that, when set, ensures that
/// we don't attempt to deliver pending messages before destorying the
/// socket
pub const DO_NOT_LINGER: i32 = 0;

/// Socket to be used for connecting to peer on the network
pub struct OutgoingSocket {
	socket: zmq::Socket,
}

impl OutgoingSocket {
	pub fn new(context: &zmq::Context, key: &KeyPair) -> Self {
		let socket = context.socket(zmq::SocketType::DEALER).unwrap();

		// Discard any pending messages when disconnecting a socket
		socket.set_linger(DO_NOT_LINGER).unwrap();

		socket.set_ipv6(true).unwrap();
		socket.set_reconnect_ivl(RECONNECT_INTERVAL.as_millis() as i32).unwrap();
		socket.set_reconnect_ivl_max(RECONNECT_INTERVAL_MAX.as_millis() as i32).unwrap();
		socket.set_maxmsgsize(MAX_MESSAGE_SIZE).unwrap();
		socket
			.set_heartbeat_ivl(CONNECTION_HEARTBEAT_INTERVAL.as_millis() as i32)
			.unwrap();
		socket
			.set_heartbeat_timeout(CONNECTION_HEARTBEAT_TIMEOUT.as_millis() as i32)
			.unwrap();

		socket.set_curve_secretkey(&key.secret_key.to_bytes()).unwrap();
		socket.set_curve_publickey(key.public_key.as_bytes()).unwrap();

		OutgoingSocket { socket }
	}

	pub fn enable_socket_events(&self, monitor_endpoint: &str, flags: u16) {
		self.socket.monitor(monitor_endpoint, flags as i32).unwrap();
	}

	pub fn connect(self, peer: PeerInfo, logger: &slog::Logger) -> ConnectedOutgoingSocket {
		let socket = self.socket;
		socket.set_curve_serverkey(peer.pubkey.as_bytes()).unwrap();

		let endpoint = peer.zmq_endpoint();
		socket.connect(&endpoint).unwrap();

		slog::debug!(logger, "Connecting to peer {} at {}", peer.account_id, &endpoint,);

		ConnectedOutgoingSocket { socket, peer, logger: logger.clone() }
	}
}

pub struct ConnectedOutgoingSocket {
	socket: zmq::Socket,
	peer: PeerInfo,
	// NOTE: ZMQ sockets can technically connect to more than
	// one endpoints, so we need to provide a specific endpoint
	// when disconnecting (even though we only connect to one
	// peer with "client" sockets). We store the endpoint here
	// for this reason.
	logger: slog::Logger,
}

impl ConnectedOutgoingSocket {
	pub fn send(&self, payload: Vec<u8>) {
		// By setting the DONTWAIT option we are ensuring that the
		// messages are dropped if the buffer for this particular
		// peer is full rather than blocking the thread (this should
		// rarely even happen, and it would usually indicate that the
		// peer has been offline for a long time)
		if let Err(err) = self.socket.send(payload, zmq::DONTWAIT) {
			slog::warn!(
				self.logger,
				"Failed to send a message to {}: {}",
				self.peer.account_id,
				err
			);
		}
	}

	pub fn peer(&self) -> &PeerInfo {
		&self.peer
	}
}
