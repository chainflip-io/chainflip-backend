//! Implements ZAP (ZeroMQ Authentication Protocol server).
//! For details, see https://rfc.zeromq.org/spec/27.
//! To use, create one Authenticator instance, and call
//! run on a separate thread.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use super::{to_string, XPublicKey};

/// These values are ZMQ convention
const ZAP_AUTH_SUCCESS: &str = "200";
const ZAP_AUTH_FAILURE: &str = "400";

pub struct Authenticator {
    // NOTE: we might be able to remove this mutex
    // (not trivially though), but this field is only
    // accessed when a new peer is added/removed and
    // when a new incoming connection is received, which
    // should be relatively infrequent events.
    // NOTE: We store public keys so we don't have to
    // map to AccountIds on every connection (which would
    // require a mutex)

    // We don't use BTreeSet because XPublicKey doesn't implement Ord
    allowed_pubkeys: RwLock<HashSet<XPublicKey>>,
    logger: slog::Logger,
}

impl Authenticator {
    fn new(logger: &slog::Logger) -> Self {
        Authenticator {
            allowed_pubkeys: Default::default(),
            logger: logger.clone(),
        }
    }

    pub fn add_peer(&self, peer_pubkey: XPublicKey) {
        slog::debug!(
            self.logger,
            "Adding to list of allowed peers pubkeys: {}",
            to_string(&peer_pubkey)
        );
        self.allowed_pubkeys.write().unwrap().insert(peer_pubkey);
    }

    pub fn remove_peer(&self, peer_pubkey: XPublicKey) {
        if self.allowed_pubkeys.write().unwrap().remove(&peer_pubkey) {
            slog::debug!(
                self.logger,
                "Removed from the list of allowed pubkeys: {}",
                to_string(&peer_pubkey)
            );
        }
    }

    /// This implements the core of the ZAP protocol: parses an
    /// authentication request and provides a response
    fn process_authentication_request(&self, socket: &zmq::Socket) {
        let req = parse_request(socket);

        if self.allowed_pubkeys.read().unwrap().contains(&req.pubkey) {
            slog::debug!(
                self.logger,
                "Allowing an incoming connection for a known pubkey: {}",
                to_string(&req.pubkey)
            );
            send_auth_response(socket, &req.request_id, ZAP_AUTH_SUCCESS, &req.pubkey)
        } else {
            slog::warn!(
                self.logger,
                "Declining an incoming connection for an unknown pubkey: {}",
                to_string(&req.pubkey)
            );
            send_auth_response(socket, &req.request_id, ZAP_AUTH_FAILURE, &req.pubkey)
        }
    }

    pub fn run(self: Arc<Self>, socket: zmq::Socket) {
        slog::info!(self.logger, "Started authentication thread!");
        // TODO: ensure that the rest of the program terminates
        // if this thread panics (which is unlikely)
        loop {
            self.process_authentication_request(&socket);
        }
    }
}

struct AuthRequest {
    /// Request id, used by ZMQ internally to
    /// link requests to responses
    request_id: Vec<u8>,
    /// Authenticated public key
    pubkey: XPublicKey,
}

pub fn start_authentication_thread(
    context: zmq::Context,
    logger: &slog::Logger,
) -> Arc<Authenticator> {
    let authenticator = Arc::new(Authenticator::new(logger));

    let zap_socket = context.socket(zmq::REP).unwrap();
    zap_socket.set_linger(0).unwrap();

    // ZMQ convention is for the authentication thread
    // to listen on this endpoint
    const AUTH_ENDPOINT: &str = "inproc://zeromq.zap.01";
    zap_socket.bind(AUTH_ENDPOINT).unwrap();

    let authenticator_clone = authenticator.clone();

    std::thread::spawn(move || {
        authenticator.run(zap_socket);
    });

    authenticator_clone
}

fn parse_request(socket: &zmq::Socket) -> AuthRequest {
    let request = socket.recv_multipart(0).unwrap();

    // The requests are guaranteed to have correct structure
    // (e.g. the correct number of message parts)
    // because they can only be initiated by ZMQ itself
    assert_eq!(request.len(), 7, "ZAP requests always have 7 parts");

    // We are only interested in the following fields:
    let request_id = &request[1];
    let pubkey: [u8; 32] = request[6].clone().try_into().unwrap();

    // NOTE: the only difference between `unchecked_from` and `try_from` is
    // that the latter checks the buffer size which we don't need to do here
    let pubkey = XPublicKey::from(pubkey);

    AuthRequest {
        request_id: request_id.to_vec(),
        pubkey,
    }
}

fn send_auth_response(
    socket: &zmq::Socket,
    request_id: &[u8],
    status_code: &str,
    pubkey: &XPublicKey,
) {
    use zmq::Message;
    let status_text = Message::from(&"");
    let metadata = Message::from(&"");

    // The value is required to be utf-8 for some reason
    let user_id = Message::from(&hex::encode(pubkey.as_bytes()));
    socket
        .send_multipart(
            [
                Message::from("1.0"),
                Message::from(request_id),
                Message::from(status_code),
                status_text,
                user_id,
                metadata,
            ],
            0,
        )
        .unwrap();
}
