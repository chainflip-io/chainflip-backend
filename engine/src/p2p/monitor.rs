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
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::p2p::socket::DO_NOT_LINGER;

use super::{socket::OutgoingSocket, PeerInfo};

/// Describes peer connection to start monitoring
#[derive(Serialize, Deserialize, Debug)]
pub struct SocketToMonitor {
    /// Endpoint on which to listen for socket events
    pub endpoint: String,
    /// Account id of the peer we are attempting to connect to
    pub account_id: AccountId,
}

enum SocketType {
    /// Used to receive new sockets to monitor
    PeerReceiver,
    /// Used to receive zmq events from a socket
    PeerMonitor(AccountId),
}

pub struct MonitorHandle {
    socket: zmq::Socket,
}

impl MonitorHandle {
    pub fn start_monitoring_for(&mut self, socket_to_monitor: &OutgoingSocket, peer: &PeerInfo) {
        use rand::RngCore;

        // Generate a random id to prevent accidentally attempting
        // to bind to the same endpoint (when reconnecting, it is
        // currently possible to open a new socket while the other
        // hasn't quite been closed).
        // TODO: see if we can reuse monitor socket when reconnecting
        let random_id = rand::thread_rng().next_u64();

        let monitor_endpoint = format!("inproc://monitor-client-{}-{}", peer.account_id, random_id);

        // These are the only events we are interested in
        let flags = zmq::SocketEvent::ALL.to_raw();

        // This makes ZMQ publish socket events
        socket_to_monitor.enable_socket_events(&monitor_endpoint, flags);

        // This is how we communicate to the monitor thread to
        // start listening to the socket events
        let peer_connection = SocketToMonitor {
            account_id: peer.account_id.clone(),
            endpoint: monitor_endpoint,
        };

        let data = bincode::serialize(&peer_connection).unwrap();
        self.socket.send(data, 0).unwrap();
    }
}

/// Creates a channel that delays delivery by `delay`
fn create_delayed_reconnect_channel(
    delay: std::time::Duration,
) -> (UnboundedSender<AccountId>, UnboundedReceiver<AccountId>) {
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
    let account_id = match sockets_to_poll.remove(idx).1 {
        SocketType::PeerReceiver => {
            panic!("Peer receiver should never be removed");
        }
        SocketType::PeerMonitor(account_id) => account_id,
    };

    slog::trace!(logger, "No longer monitoring peer: {}", account_id);
}

/// Returns a socket (used by p2p control loop to send new
/// peer connections to monitor), and a receiver channel (used
/// by p2p control loop to receive commands to reconnect to the peer)
pub fn start_monitoring_thread(
    context: zmq::Context,
    logger: &slog::Logger,
) -> (MonitorHandle, UnboundedReceiver<AccountId>) {
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

            // NOTE: we read in reverse order to ensure that
            // removing elements is safe
            for idx in readable_indexes.iter().rev() {
                let (socket, socket_type) = &sockets_to_poll[*idx];
                // NOTE: we only read from each socket once even though
                // there may be more than one event ready (the remaining
                // events, if any, will simply be read in the next iteration)
                let message = socket.recv_multipart(0).unwrap();
                match socket_type {
                    SocketType::PeerReceiver => {
                        let SocketToMonitor {
                            account_id,
                            endpoint,
                        } = bincode::deserialize(&message[0].to_vec()).unwrap();

                        slog::info!(logger, "Start monitoring peer {}", &account_id);

                        // Create a monitoring socket for the new peer
                        let monitor_socket = context.socket(zmq::PAIR).unwrap();
                        monitor_socket.set_linger(DO_NOT_LINGER).unwrap();
                        monitor_socket.connect(&endpoint).unwrap();

                        sockets_to_poll.push((monitor_socket, SocketType::PeerMonitor(account_id)));
                    }
                    SocketType::PeerMonitor(account_id) => {
                        // We are only interested in the event id (the first two bytes of the first message)
                        let event_id = u16::from_le_bytes(message[0][0..2].try_into().unwrap());
                        match zmq::SocketEvent::from_raw(event_id) {
                            zmq::SocketEvent::HANDSHAKE_FAILED_AUTH => {
                                slog::warn!(
                                    logger,
                                    "Socket event: authentication failed with {}",
                                    account_id
                                );
                                reconnect_sender.send(account_id.clone()).unwrap();
                            }
                            zmq::SocketEvent::MONITOR_STOPPED => {
                                // This event indicates that the socket of interest has
                                // been dropped/closed, so we remove any reference to it on our
                                // side too.
                                // Note that this only happens if we are already reconnecting
                                // (with a new socket) or if we were told by SC to remove the
                                // peer, so there is no danger that we won't connect because
                                // the monitoring stopped.
                                stop_monitoring_for_peer(&mut sockets_to_poll, *idx, &logger);
                            }
                            zmq::SocketEvent::HANDSHAKE_SUCCEEDED => {
                                // We no longer need to monitor the socket once we have
                                // successfully connected (and authenticated) to the peer
                                slog::trace!(
                                    logger,
                                    "Socket event: authentication success with {}",
                                    account_id
                                );
                                stop_monitoring_for_peer(&mut sockets_to_poll, *idx, &logger);
                            }
                            zmq::SocketEvent::HANDSHAKE_FAILED_PROTOCOL => {
                                slog::trace!(
                                    logger,
                                    "Socket event: HANDSHAKE_FAILED_PROTOCOL {}",
                                    account_id
                                );
                            }
                            zmq::SocketEvent::HANDSHAKE_FAILED_NO_DETAIL => {
                                slog::trace!(
                                    logger,
                                    "Socket event: HANDSHAKE_FAILED_NO_DETAIL {}",
                                    account_id
                                );
                            }
                            zmq::SocketEvent::DISCONNECTED => {
                                slog::trace!(logger, "Socket event: DISCONNECTED {}", account_id);
                            }
                            zmq::SocketEvent::CLOSE_FAILED => {
                                slog::trace!(logger, "Socket event: CLOSE_FAILED {}", account_id);
                            }
                            zmq::SocketEvent::CLOSED => {
                                slog::trace!(logger, "Socket event: CLOSED {}", account_id);
                            }
                            zmq::SocketEvent::ACCEPT_FAILED => {
                                slog::trace!(logger, "Socket event: ACCEPT_FAILED {}", account_id);
                            }
                            zmq::SocketEvent::ACCEPTED => {
                                slog::trace!(logger, "Socket event: ACCEPTED {}", account_id);
                            }
                            zmq::SocketEvent::CONNECT_RETRIED => {
                                slog::trace!(
                                    logger,
                                    "Socket event: CONNECT_RETRIED {}",
                                    account_id
                                );
                            }
                            zmq::SocketEvent::CONNECT_DELAYED => {
                                slog::trace!(
                                    logger,
                                    "Socket event: CONNECT_DELAYED {}",
                                    account_id
                                );
                            }
                            zmq::SocketEvent::CONNECTED => {
                                slog::trace!(logger, "Socket event: CONNECTED {}", account_id);
                            }
                            unexpected_event => {
                                slog::trace!(
                                    logger,
                                    "Socket event: ({}) {}",
                                    unexpected_event.to_raw(),
                                    account_id
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
