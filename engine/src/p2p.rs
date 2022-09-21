mod auth;
mod monitor;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;

use auth::Authenticator;
use serde::{Deserialize, Serialize};
use sp_core::ed25519;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use x25519_dalek::StaticSecret;

use crate::logging::COMPONENT_KEY;
use crate::multisig_p2p::OutgoingMultisigStageMessages;

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

type EdPublicKey = ed25519::Public;
type XPublicKey = x25519_dalek::PublicKey;

pub struct KeyPair {
    pub public_key: XPublicKey,
    pub secret_key: StaticSecret,
}

struct SocketInfo {
    socket: zmq::Socket,
    // NOTE: ZMQ sockets can technically connect to more than
    // one endpoints, so we need to provide a specific endpoint
    // when disconnecting (even though we only connect to one
    // peer with "client" sockets). We store the endpoint here
    // for this reason.
    endpoint: String,
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
    pub port: u16,
}

impl PeerInfo {
    pub fn new(account_id: AccountId, ed_public_key: EdPublicKey, ip: Ipv6Addr, port: u16) -> Self {
        let ed_public_key = ed25519_dalek::PublicKey::from_bytes(&ed_public_key.0).unwrap();
        let x_public_key = ed25519_public_key_to_x25519_public_key(&ed_public_key);

        PeerInfo {
            account_id,
            pubkey: x_public_key,
            ip,
            port,
        }
    }
}

impl std::fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "PeerInfo {{ account_id: {}, pubkey: {}, ip: {}, port: {} }}",
            self.account_id,
            to_string(&self.pubkey),
            self.ip,
            self.port,
        )
    }
}

/// Used to track "registration" status on the network
enum PeerState {
    /// The node is not yet known to the network (its peer info
    /// may not be known to the network yet)
    /// (Stores future peers to connect to when then node is registered)
    Pending(Vec<PeerInfo>),
    /// The node is registered, i.e. its peer info has been
    /// recorded/updated
    Registered,
}

fn ed25519_secret_key_to_x25519_secret_key(
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
    let ed_point = CompressedEdwardsY::from_slice(&ed25519_pk.to_bytes())
        .decompress()
        .unwrap();
    let x_point = ed_point.to_montgomery();

    x25519_dalek::PublicKey::from(x_point.to_bytes())
}

fn to_string(pk: &XPublicKey) -> String {
    hex::encode(pk.as_bytes())
}
/// The state a nodes needs for p2p
pub struct P2PContext {
    zmq_context: zmq::Context,
    /// Our own key, used for initiating and accepting secure connections
    key: KeyPair,
    /// A handle to the authenticator thread that can be used to make changes to the
    /// list of allowed peers
    authenticator: Arc<Authenticator>,
    /// NOTE: The mapping is from AccountId because we want to optimise for message
    /// sending, which uses AccountId
    active_connections: BTreeMap<AccountId, SocketInfo>,
    /// NOTE: this is used for incoming messages when we want to map them to account_id
    /// NOTE: we don't use BTreeMap here because XPublicKey doesn't implement Ord.
    x25519_to_account_id: HashMap<XPublicKey, AccountId>,
    /// Channel through which we send incoming messages to the multisig
    incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
    /// This is how we communicate with the "monitor" thread
    monitor_handle: monitor::MonitorHandle,
    /// Our own "registration" status on the network
    state: PeerState,
    our_account_id: AccountId,
    logger: slog::Logger,
}

pub fn start(
    node_key: &ed25519_dalek::Keypair,
    cfe_port: u16,
    peer_infos: Vec<PeerInfo>,
    our_account_id: AccountId,
    logger: &slog::Logger,
) -> (
    UnboundedSender<OutgoingMultisigStageMessages>,
    UnboundedSender<PeerUpdate>,
    UnboundedReceiver<(AccountId, Vec<u8>)>,
    impl Future<Output = ()>,
) {
    let secret_key = ed25519_secret_key_to_x25519_secret_key(&node_key.secret);

    let public_key: x25519_dalek::PublicKey = (&secret_key).into();
    slog::debug!(
        logger,
        "Our derived x25519 pubkey: {:?}",
        to_string(&public_key)
    );

    let key = KeyPair {
        public_key,
        secret_key,
    };

    // We listen on all interfaces
    let ip: Ipv4Addr = "0.0.0.0".parse().unwrap();

    P2PContext::start(key, ip, cfe_port, peer_infos, our_account_id, logger)
}

fn set_general_client_socket_options(socket: &zmq::Socket) {
    // Discard any pending messages when disconnecting a socket
    socket.set_linger(0).unwrap();

    socket.set_ipv6(true).unwrap();
    socket
        .set_reconnect_ivl(RECONNECT_INTERVAL.as_millis() as i32)
        .unwrap();
    socket
        .set_reconnect_ivl_max(RECONNECT_INTERVAL_MAX.as_millis() as i32)
        .unwrap();
    socket.set_maxmsgsize(MAX_MESSAGE_SIZE).unwrap();
    socket
        .set_heartbeat_ivl(CONNECTION_HEARTBEAT_INTERVAL.as_millis() as i32)
        .unwrap();
    socket
        .set_heartbeat_timeout(CONNECTION_HEARTBEAT_TIMEOUT.as_millis() as i32)
        .unwrap();
}

impl P2PContext {
    fn start(
        key: KeyPair,
        ip: Ipv4Addr,
        port: u16,
        current_peers: Vec<PeerInfo>,
        our_account_id: AccountId,
        logger: &slog::Logger,
    ) -> (
        UnboundedSender<OutgoingMultisigStageMessages>,
        UnboundedSender<PeerUpdate>,
        UnboundedReceiver<(AccountId, Vec<u8>)>,
        impl Future<Output = ()>,
    ) {
        let zmq_context = zmq::Context::new();

        // TODO: consider if we need to change the default limit for open sockets
        // (the default is 1024)

        // TODO: consider keeping track of "last activity" on any outgoing
        // socket connection and disconnecting inactive peers (see proxy_expire_idle_peers
        // in OxenMQ)

        let logger = logger.new(slog::o!(COMPONENT_KEY => "p2p"));

        let authenticator = auth::start_authentication_thread(zmq_context.clone(), &logger);

        let (incoming_message_sender, incoming_message_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let (monitor_handle, reconnect_receiver) =
            monitor::start_monitoring_thread(zmq_context.clone(), &logger);

        let mut context = P2PContext {
            zmq_context,
            key,
            monitor_handle,
            authenticator,
            active_connections: Default::default(),
            x25519_to_account_id: Default::default(),
            incoming_message_sender,
            our_account_id,
            state: PeerState::Pending(vec![]),
            logger,
        };

        for peer_info in current_peers {
            context.add_or_update_peer(peer_info);
        }

        let incoming_message_receiver_ed25519 = context.start_listening_thread(ip, port);

        let (out_msg_sender, out_msg_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

        let fut = context.control_loop(
            out_msg_receiver,
            incoming_message_receiver_ed25519,
            peer_update_receiver,
            reconnect_receiver,
        );

        (
            out_msg_sender,
            peer_update_sender,
            incoming_message_receiver,
            fut,
        )
    }

    async fn control_loop(
        mut self,
        mut outgoing_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
        mut incoming_message_receiver: UnboundedReceiver<(XPublicKey, Vec<u8>)>,
        mut peer_update_receiver: UnboundedReceiver<PeerUpdate>,
        mut reconnect_receiver: UnboundedReceiver<PeerInfo>,
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
                    self.on_incoming_message(pubkey, payload);
                }
                Some(peer_info) = reconnect_receiver.recv() => {
                    self.reconnect_to_peer(peer_info);
                }
            }
        }
    }

    fn send_messages(&self, messages: OutgoingMultisigStageMessages) {
        match messages {
            OutgoingMultisigStageMessages::Broadcast(account_ids, payload) => {
                for acc_id in account_ids {
                    self.send_message(acc_id, payload.clone());
                }
            }
            OutgoingMultisigStageMessages::Private(messages) => {
                for (acc_id, payload) in messages {
                    self.send_message(acc_id, payload);
                }
            }
        }
    }

    fn send_message(&self, account_id: AccountId, payload: Vec<u8>) {
        match self.active_connections.get(&account_id) {
            Some(SocketInfo { socket, .. }) => {
                // By setting the DONTWAIT option we are ensuring that the
                // messages are dropped if the buffer for this particular
                // peer is full rather than blocking the thread (this should
                // rarely even happen, and it would usually indicate that the
                // peer has been offline for a long time)
                if let Err(err) = socket.send(payload, zmq::DONTWAIT) {
                    slog::warn!(
                        self.logger,
                        "Failed to send a message to {}: {}",
                        account_id,
                        err
                    );
                } else {
                    slog::trace!(self.logger, "Sent a message to: {}", account_id);
                }
            }
            None => {
                slog::warn!(
                    self.logger,
                    "Failed to send message. Peer not registered: {}",
                    account_id
                )
            }
        }
    }

    fn on_peer_update(&mut self, update: PeerUpdate) {
        match update {
            PeerUpdate::Registered(peer_info) => self.add_or_update_peer(peer_info),
            PeerUpdate::Deregistered(account_id, pubkey) => self.remove_peer(account_id, pubkey),
        }
    }

    fn on_incoming_message(&mut self, pubkey: XPublicKey, payload: Vec<u8>) {
        if let Some(acc_id) = self.x25519_to_account_id.get(&pubkey) {
            slog::trace!(self.logger, "Received a message from {}", acc_id);
            self.incoming_message_sender
                .send((acc_id.clone(), payload))
                .unwrap();
        } else {
            slog::warn!(
                self.logger,
                "Received a message for an unknown ed25519 key: {}",
                to_string(&pubkey)
            );
        }
    }

    /// Removing a peer means: (1) removing it from the list of allowed nodes,
    /// (2) disconnecting our "client" socket with that node, (3) removing
    /// any references to it in local state (mappings)
    fn remove_peer(&mut self, account_id: AccountId, ed_public_key: EdPublicKey) {
        // NOTE: There is no (trivial) way to disconnect peers that are
        // already connected to our listening ZMQ socket, we can only
        // prevent future connections from being established and rely
        // on peer from disconnecting from "client side".
        // TODO: ensure that stale/inactive connections are terminated

        let public_key = {
            let pk = ed25519_dalek::PublicKey::from_bytes(&ed_public_key.0).unwrap();
            ed25519_public_key_to_x25519_public_key(&pk)
        };

        self.authenticator.remove_peer(public_key);

        match self.x25519_to_account_id.remove(&public_key) {
            Some(stored_account_id) => {
                if account_id != stored_account_id {
                    slog::warn!(
                        self.logger,
                        "Stored account id {} does not match provided {} in the request to remove peer",
                        stored_account_id, account_id
                    );
                }
            }
            None => {
                slog::warn!(
                    self.logger,
                    "No account id matches the ed25519 key provided in the request to remove peer: {}",
                    to_string(&public_key)
                );
                return;
            }
        };

        if let Some(SocketInfo { socket, endpoint }) = self.active_connections.remove(&account_id) {
            match socket.disconnect(&endpoint) {
                Ok(()) => {
                    slog::debug!(self.logger, "Disconnected from peer: {}", account_id);
                }
                Err(err) => {
                    slog::warn!(
                        self.logger,
                        "Could not disconnect from peer: {}, ({})",
                        account_id,
                        err
                    );
                }
            }
        }
    }

    fn reconnect_to_peer(&mut self, peer: PeerInfo) {
        slog::info!(self.logger, "Reconnecting to peer: {}", peer.account_id);
        if let Some(socket_info) = self.active_connections.remove(&peer.account_id) {
            socket_info
                .socket
                .disconnect(&socket_info.endpoint)
                .unwrap();
        } else {
            panic!("Can only reconnect to existing peers!");
        }

        self.connect_to_peer(peer)
    }

    fn connect_to_peer(&mut self, peer: PeerInfo) {
        slog::info!(self.logger, "Connecting to: {}", peer.port);

        let account_id = peer.account_id.clone();

        let socket = self.zmq_context.socket(zmq::SocketType::DEALER).unwrap();

        set_general_client_socket_options(&socket);

        socket
            .set_curve_secretkey(&self.key.secret_key.to_bytes())
            .unwrap();
        socket
            .set_curve_publickey(self.key.public_key.as_bytes())
            .unwrap();
        socket.set_curve_serverkey(peer.pubkey.as_bytes()).unwrap();

        // TODO: we may want to use routing ids based on the pubkey to allow connection reuse
        // when the peer reconnects

        self.monitor_handle.start_monitoring_for(&peer);

        let endpoint = format!("tcp://[{}]:{}", peer.ip, peer.port);
        socket.connect(&endpoint).unwrap();

        slog::debug!(
            self.logger,
            "Connecting to peer {} at {}",
            account_id,
            &endpoint
        );

        assert!(self
            .active_connections
            .insert(account_id, SocketInfo { socket, endpoint })
            .is_none());
    }

    fn add_or_update_peer(&mut self, peer: PeerInfo) {
        slog::debug!(self.logger, "Received new peer info: {}", peer);
        if self.active_connections.contains_key(&peer.account_id) {
            slog::debug!(
                self.logger,
                "Account id {} is already registered, updating",
                &peer.account_id
            );
        }

        if peer.account_id == self.our_account_id {
            if let PeerState::Pending(peers) = &mut self.state {
                let peers = std::mem::take(peers);
                // Connect to all outstanding peers
                for peer in peers {
                    self.connect_to_peer(peer)
                }
                self.state = PeerState::Registered;
            };
        } else {
            let peer_pubkey = &peer.pubkey;
            self.authenticator.add_peer(*peer_pubkey);

            slog::trace!(
                self.logger,
                "Adding x25519 to account id mapping: {} -> {}",
                &peer.account_id,
                to_string(peer_pubkey)
            );

            self.x25519_to_account_id
                .insert(*peer_pubkey, peer.account_id.clone());

            match &mut self.state {
                PeerState::Pending(peers) => {
                    // Not ready to start connecting to peers yet
                    slog::info!(self.logger, "Delaying connecting to {}", peer.account_id);
                    peers.push(peer);
                }
                PeerState::Registered => {
                    self.connect_to_peer(peer);
                }
            }
        }
    }

    /// Start listening for incoming p2p messages on a separate thread
    fn start_listening_thread(
        &mut self,
        ip: Ipv4Addr,
        port: u16,
    ) -> UnboundedReceiver<(XPublicKey, Vec<u8>)> {
        let socket = self.zmq_context.socket(zmq::SocketType::ROUTER).unwrap();

        socket.set_router_mandatory(true).unwrap();
        socket.set_router_handover(true).unwrap();
        socket.set_curve_server(true).unwrap();
        socket
            .set_curve_secretkey(&self.key.secret_key.to_bytes())
            .unwrap();

        let endpoint = format!("tcp://{}:{}", ip, port);
        slog::info!(
            self.logger,
            "Started listening for incoming p2p connections on: {}",
            endpoint
        );

        socket.bind(&endpoint).expect("invalid endpoint");

        let (incoming_message_sender, incoming_message_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        // This OS thread is for incoming messages
        // TODO: combine this with the authentication thread?
        std::thread::spawn(move || loop {
            // Sender id is automatically attached by ZMQ,
            // we are not interested in it
            let _sender_id = socket.recv_msg(0).unwrap();

            let mut msg = socket.recv_msg(0).unwrap();

            // This value is ZMQ convention for the public
            // key of message's origin
            const PUBLIC_KEY_TAG: &str = "User-Id";
            let pubkey = msg.gets(PUBLIC_KEY_TAG).expect("pubkey is always present");

            let pubkey: [u8; 32] = hex::decode(pubkey).unwrap().try_into().unwrap();
            let pubkey = XPublicKey::from(pubkey);

            incoming_message_sender
                .send((pubkey, msg.to_vec()))
                .unwrap();
        });

        incoming_message_receiver
    }
}
