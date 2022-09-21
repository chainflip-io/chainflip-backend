//! This module implements the functionality to monitor "client" ZMQ
//! sockets. ZMQ has an unfortunate design where sockets don't automatically
//! reconnect if they get an authentication error (unlike the case where
//! the "server" is simply unreachable).
//! At Chainflip this will most likely happen due to a race condition
//! where the node's info has not yet propagated to all peers, and we
//! almost certainly want to attempt to reconnect almost immediately.
//! The workaround is to "subscribe" to socket events and reconnect
//! manually on receiving `HANDSHAKE_FAILED_AUTH` error.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::PeerInfo;

/// Describes peer connection to start monitoring
#[derive(Serialize, Deserialize, Debug)]
pub struct SocketToMonitor {
    /// Endpoint on which to listen for socket events
    pub endpoint: String,
    /// Information used to make another connection
    /// attempt if necessary
    pub peer_info: PeerInfo,
}

enum SocketType {
    /// Used to receive new sockets to monitor
    PeerReceiver,
    /// Used to receive zmq events from a socket
    PeerMonitor(PeerInfo),
}

pub struct MonitorHandle {
    socket: zmq::Socket,
}

impl MonitorHandle {
    pub fn start_monitoring_for(&mut self, peer: &PeerInfo) {
        use rand::RngCore;

        // Generate a random id to prevent accidentally attempting
        // to bind to the same endpoint (when reconnecting, it is
        // currently possible to open a new socket while the other
        // hasn't quite been closed).
        // TODO: see if we can reuse monitor socket when reconnecting
        let random_id = rand::thread_rng().next_u64();

        let monitor_endpoint = format!("inproc://monitor-client-{}-{}", peer.account_id, random_id);

        // These are the only events we are interested in
        let flags = zmq::SocketEvent::HANDSHAKE_FAILED_AUTH.to_raw()
            | zmq::SocketEvent::MONITOR_STOPPED.to_raw()
            | zmq::SocketEvent::HANDSHAKE_SUCCEEDED.to_raw();

        // This makes ZMQ publish socket events
        self.socket
            .monitor(&monitor_endpoint, flags as i32)
            .unwrap();

        // This is how we communicate to the monitor thread to
        // start listening to the socket events
        let peer_connection = SocketToMonitor {
            peer_info: peer.clone(),
            endpoint: monitor_endpoint,
        };

        let data = bincode::serialize(&peer_connection).unwrap();
        self.socket.send(data, 0).unwrap();
    }
}

/// Creates a channel that delays delivery by `delay`
fn create_delayed_reconnect_channel(
    delay: std::time::Duration,
) -> (UnboundedSender<PeerInfo>, UnboundedReceiver<PeerInfo>) {
    let (reconnect_sender, mut reconnect_receiver) = tokio::sync::mpsc::unbounded_channel();

    let (delayed_reconnect_sender, delayed_reconnect_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(peer_info) = reconnect_receiver.recv().await {
            let sender = delayed_reconnect_sender.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                sender.send(peer_info).unwrap();
            });
        }
    });

    (reconnect_sender, delayed_reconnect_receiver)
}

fn stop_monitoring_for_peer(
    sockets_to_poll: &mut Vec<(zmq::Socket, SocketType)>,
    idx: usize,
    logger: &slog::Logger,
) {
    let peer_info = match sockets_to_poll.remove(idx).1 {
        SocketType::PeerReceiver => {
            panic!("Peer receiver should never be removed");
        }
        SocketType::PeerMonitor(peer_info) => peer_info,
    };

    slog::trace!(
        logger,
        "No longer monitoring peer: {}",
        peer_info.account_id
    );
}

/// Returns a socket (used by p2p control loop to send new
/// peer connections to monitor), and a receiver channel (used
/// by p2p control loop to receive commands to reconnect to the peer)
pub fn start_monitoring_thread(
    context: zmq::Context,
    logger: &slog::Logger,
) -> (MonitorHandle, UnboundedReceiver<PeerInfo>) {
    let logger = logger.clone();

    // This essentially opens a (ZMQ) channel that the monitor thread
    // uses to receive new peer sockets to monitor
    const PEER_INFO_ENDPOINT: &str = "inproc://peer_info_for_monitoring";
    let monitor_socket = context.socket(zmq::PUSH).unwrap();
    monitor_socket.connect(PEER_INFO_ENDPOINT).unwrap();

    // A "delayed" channel is used to rate limit reconnection attempts
    // TODO: a more elegant solution with exponential back-off strategy
    let (reconnect_sender, reconnect_receiver) =
        create_delayed_reconnect_channel(std::time::Duration::from_secs(1));

    std::thread::spawn(move || {
        let peer_receiver = context.socket(zmq::PULL).unwrap();
        peer_receiver.bind(PEER_INFO_ENDPOINT).unwrap();

        let mut sockets_to_poll = vec![(peer_receiver, SocketType::PeerReceiver)];

        loop {
            // While not ideal, we rebuild this vector on the fly
            // because (1) poll items contain pointers to sockets
            // and don't expect them to move as we add/remove sockets
            // and (2) this makes it easier to keep the mapping
            // from poll items back to sockets correct
            let mut poll_items: Vec<_> = sockets_to_poll
                .iter()
                .map(|socket| socket.0.as_poll_item(zmq::POLLIN))
                .collect();

            slog::trace!(logger, "Items to monitor total: {}", poll_items.len());

            // Block until one or more sockets are "readable"
            let _count = zmq::poll(&mut poll_items, -1);

            let readable_indexes: Vec<_> = poll_items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.is_readable())
                .map(|(idx, _)| idx)
                .collect();

            for idx in readable_indexes {
                let (socket, socket_type) = &sockets_to_poll[idx];
                // NOTE: we only read from each socket once even though
                // there may be more than one event ready (the remaining
                // events, if any, will simply be read in the next iteration)
                let message = socket.recv_multipart(0).unwrap();
                match socket_type {
                    SocketType::PeerReceiver => {
                        let SocketToMonitor {
                            peer_info,
                            endpoint,
                        } = bincode::deserialize(&message[0].to_vec()).unwrap();

                        slog::info!(logger, "Start monitoring peer {}", &peer_info.account_id);

                        // Create a monitoring socket for the new peer
                        let monitor_socket = context.socket(zmq::PAIR).unwrap();
                        monitor_socket.set_linger(0).unwrap();
                        monitor_socket.connect(&endpoint).unwrap();

                        sockets_to_poll.push((monitor_socket, SocketType::PeerMonitor(peer_info)));
                    }
                    SocketType::PeerMonitor(peer_info) => {
                        // We are only interested in the event id (the first two bytes of the first message)
                        let event_id = u16::from_le_bytes(message[0][0..2].try_into().unwrap());
                        match zmq::SocketEvent::from_raw(event_id) {
                            zmq::SocketEvent::HANDSHAKE_FAILED_AUTH => {
                                slog::warn!(
                                    logger,
                                    "Socket event: authentication failed with {}",
                                    peer_info.account_id
                                );
                                reconnect_sender.send(peer_info.clone()).unwrap();
                            }
                            zmq::SocketEvent::MONITOR_STOPPED => {
                                // This event usually indicates that the socket of interest
                                // has been closed, so we remove any reference to it on our
                                // side too
                                stop_monitoring_for_peer(&mut sockets_to_poll, idx, &logger);
                            }
                            zmq::SocketEvent::HANDSHAKE_SUCCEEDED => {
                                // We no longer need to monitor the socket once we have
                                // successfully connected (and authenticated) to the peer
                                slog::trace!(
                                    logger,
                                    "Socket event: authentication success with {}",
                                    peer_info.account_id
                                );
                                stop_monitoring_for_peer(&mut sockets_to_poll, idx, &logger);
                            }
                            unknown_event => {
                                slog::error!(
                                    logger,
                                    "MONITOR: unexpected socket event: {}",
                                    unknown_event.to_raw()
                                );
                            }
                        }
                    }
                }
            }
        }
    });

    (
        MonitorHandle {
            socket: monitor_socket,
        },
        reconnect_receiver,
    )
}
