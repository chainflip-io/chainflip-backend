//! Chainflip P2P layer.
//!
//! This code allows this node's CFE to communicate with other node's CFEs using substrate's
//! existing p2p network. We give substrate a RpcRequestHandler object which substrate uses to
//! process Rpc requests, and we create and run a background future that processes incoming p2p
//! messages and sends them to any Rpc subscribers we have (Our local CFE).

use anyhow::{anyhow, Context};
use cf_utilities::{make_periodic_tick, Port};
use ipc_channel::ipc::TryRecvError;
pub use sc_network::PeerId;

use async_trait::async_trait;
use futures::{stream, Stream, StreamExt};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_network::{multiaddr, Event, ExHashT, NetworkService};
use serde::{self, Deserialize, Serialize};
use sp_runtime::{sp_std::sync::Arc, traits::Block as BlockT};
use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet},
	marker::Send,
	net::Ipv6Addr,
	pin::Pin,
	sync::Mutex,
	time::Duration,
};
use tokio::sync::{
	mpsc::{error::TrySendError, Sender as TokioSender},
	oneshot,
};

#[cfg(test)]
use mockall::automock;

/// The identifier for our protocol, required to distinguish it from other protocols running on the
/// substrate p2p network.
pub const CHAINFLIP_P2P_PROTOCOL_NAME: Cow<str> = Cow::Borrowed("/chainflip-protocol");
pub const RETRY_SEND_INTERVAL: Duration = Duration::from_secs(30);
const RETRY_SEND_ATTEMPTS: usize = 10;

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
		fallback_names: Vec::new(),
	}
}

/// A struct to encode a PeerId so it is Serializable and Deserializable
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdTransferable(Vec<u8>);

impl TryInto<PeerId> for PeerIdTransferable {
	type Error = anyhow::Error;

	fn try_into(self) -> std::result::Result<PeerId, Self::Error> {
		PeerId::from_bytes(&self.0[..]).map_err(Into::into)
	}
}

impl From<&PeerId> for PeerIdTransferable {
	fn from(peer_id: &PeerId) -> Self {
		Self(peer_id.to_bytes())
	}
}

pub struct RpcRequestHandler<P2PNetworkService: PeerNetwork> {
	message_sender_spawner: sc_service::SpawnTaskHandle,
	shared_state: Arc<SharedState>,
	p2p_network_service: Arc<P2PNetworkService>,
	retry_send_period: Duration,
}

type ReservedPeers = BTreeMap<PeerId, (Port, Ipv6Addr, TokioSender<Vec<u8>>)>;

struct IpcState {
	incoming_ipc_sender: ipc_channel::ipc::IpcSender<(PeerIdTransferable, Vec<u8>)>,
	_outgoing_receiver_shutdown_sender: oneshot::Sender<()>,
}

#[derive(Default)]
struct SharedState {
	reserved_peers: Mutex<ReservedPeers>,
	ipc_state: Mutex<Option<IpcState>>,
}

/// An abstration of the underlying network of peers.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peer(&self, peer_id: PeerId, port: Port, address: Ipv6Addr);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peer(&self, peer_id: PeerId);
	/// Write notification to network to peer id, over protocol
	async fn try_send_notification(&self, who: PeerId, message: &[u8]) -> bool;
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>>;
}

/// An implementation of [PeerNetwork] using substrate's libp2p-based `NetworkService`.
#[async_trait]
impl<B: BlockT, H: ExHashT> PeerNetwork for NetworkService<B, H> {
	fn reserve_peer(&self, peer_id: PeerId, port: Port, address: Ipv6Addr) {
		if let Err(err) = self.add_peers_to_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			std::iter::once(
				[
					multiaddr::Protocol::Ip6(address),
					multiaddr::Protocol::Tcp(port),
					multiaddr::Protocol::P2p(peer_id.into()),
				]
				.iter()
				.cloned()
				.collect::<multiaddr::Multiaddr>(),
			)
			.collect(),
		) {
			log::error!(target: "p2p", "add_peers_to_reserved_set failed: {}", err);
		}
	}

	fn remove_reserved_peer(&self, peer_id: PeerId) {
		self.remove_peers_from_reserved_set(CHAINFLIP_P2P_PROTOCOL_NAME, vec![peer_id])
	}

	async fn try_send_notification(&self, target: PeerId, message: &[u8]) -> bool {
		async move {
			self.notification_sender(target, CHAINFLIP_P2P_PROTOCOL_NAME)
				.ok()?
				.ready()
				.await
				.ok()?
				.send(message)
				.ok()?;
			Some(())
		}
		.await
		.is_some()
	}

	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
		Box::pin(self.event_stream("network-chainflip"))
	}
}

#[rpc(client, server, namespace = "p2p")]
pub trait P2PValidatorNetworkNodeRpcApi {
	/// Connect to authorities and disconnect from old authorities
	#[method(name = "set_peers")]
	fn set_peers(&self, peers: Vec<(PeerIdTransferable, Port, Ipv6Addr)>) -> RpcResult<u64>;

	/// Connect to a authority
	#[method(name = "add_peer")]
	fn add_peer(
		&self,
		peer_id: PeerIdTransferable,
		port: Port,
		address: Ipv6Addr,
	) -> RpcResult<u64>;

	/// Disconnect from a authority
	#[method(name = "remove_peer")]
	fn remove_peer(&self, peer_id: PeerIdTransferable) -> RpcResult<u64>;

	#[method(name = "setup_ipc_connections")]
	fn setup_ipc_connections(&self, server_name: String) -> RpcResult<u64>;
}

impl<PN: PeerNetwork + Send + Sync + 'static> RpcRequestHandler<PN> {
	fn update_peer_mapping(
		&self,
		reserved_peers: &mut std::sync::MutexGuard<ReservedPeers>,
		peer_id: PeerId,
		port: Port,
		ip_address: Ipv6Addr,
	) -> bool {
		if let Some((existing_port, existing_ip_address, _message_sender)) =
			reserved_peers.get_mut(&peer_id)
		{
			if *existing_port != port || *existing_ip_address != ip_address {
				*existing_port = port;
				*existing_ip_address = ip_address;
				// TODO Check that removing then adding a peer is enough
				self.p2p_network_service.remove_reserved_peer(peer_id);
				self.p2p_network_service.reserve_peer(peer_id, port, ip_address);
				true
			} else {
				false
			}
		} else {
			let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
			reserved_peers.insert(peer_id, (port, ip_address, sender));
			let p2p_network_service = self.p2p_network_service.clone();
			let retry_send_period = self.retry_send_period;
			self.message_sender_spawner
				.spawn("cf-peer-message-sender", "chainflip", async move {
					while let Some(message) = receiver.recv().await {
						// TODO: Logic here can be improved to effectively only send when you have a
						// strong indication it will succeed (By using the connect and disconnect
						// notifications) Also it is not ideal to drop new messages, better to drop
						// old messages.
						let mut attempts = RETRY_SEND_ATTEMPTS;
						while attempts > 0 {
							if p2p_network_service.try_send_notification(peer_id, &message).await {
								break
							} else {
								attempts -= 1;
								tokio::time::sleep(retry_send_period).await;
							}
						}
						if 0 == attempts {
							log::info!("Dropping message for peer {}", peer_id);
						}
					}
				});
			self.p2p_network_service.reserve_peer(peer_id, port, ip_address);
			true
		}
	}
}

pub fn new_ipc_stream<T: serde::Serialize + serde::de::DeserializeOwned>(
	ipc_receiver: ipc_channel::ipc::IpcReceiver<T>,
) -> impl Unpin + Stream<Item = T> {
	Box::pin(
		stream::unfold(
			(false, make_periodic_tick(Duration::from_millis(100), true), ipc_receiver),
			|(last_received, mut periodic_tick_stream, ipc_receiver)| async move {
				if !last_received {
					periodic_tick_stream.tick().await;
				}
				match ipc_receiver.try_recv() {
					Ok(t) => Some((Some(t), (true, periodic_tick_stream, ipc_receiver))),
					Err(TryRecvError::Empty) =>
						Some((None, (false, periodic_tick_stream, ipc_receiver))),
					Err(TryRecvError::IpcError(_)) => None,
				}
			},
		)
		.filter_map(std::future::ready),
	)
}

pub fn new_p2p_network_node<PN: PeerNetwork + Send + Sync + 'static>(
	p2p_network_service: Arc<PN>,
	message_sender_spawner: sc_service::SpawnTaskHandle,
	retry_send_period: Duration,
) -> (Arc<RpcRequestHandler<PN>>, impl futures::Future<Output = ()>) {
	let shared_state = Arc::new(SharedState::default());

	(
		// RPC Request Handler
		{
			impl<PN: PeerNetwork + Send + Sync + 'static> P2PValidatorNetworkNodeRpcApiServer
				for Arc<RpcRequestHandler<PN>>
			{
				/// Connect to authorities
				fn set_peers(
					&self,
					peers: Vec<(PeerIdTransferable, Port, Ipv6Addr)>,
				) -> RpcResult<u64> {
					let peers = peers
						.into_iter()
						.map(|(peer_id, port, address)| {
							RpcResult::Ok((peer_id.try_into()?, (port, address)))
						})
						.collect::<std::result::Result<BTreeMap<_, _>, _>>()?;

					let mut reserved_peers = self.shared_state.reserved_peers.lock().unwrap();
					reserved_peers.retain(|peer_id, _| {
						if peers.contains_key(peer_id) {
							true
						} else {
							self.p2p_network_service.remove_reserved_peer(*peer_id);
							false
						}
					});

					// TODO: Investigate why adding/removing multiple reserved peers in a single
					// reserve_peers call doesn't work
					for (peer_id, (port, ip_address)) in peers {
						self.update_peer_mapping(&mut reserved_peers, peer_id, port, ip_address);
					}

					log::info!(
						"Set {} reserved peers (Total Reserved: {})",
						CHAINFLIP_P2P_PROTOCOL_NAME,
						reserved_peers.len()
					);

					Ok(200)
				}

				/// Connect to an authority
				fn add_peer(
					&self,
					peer_id: PeerIdTransferable,
					port: Port,
					ip_address: Ipv6Addr,
				) -> RpcResult<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut reserved_peers = self.shared_state.reserved_peers.lock().unwrap();
					if self.update_peer_mapping(&mut reserved_peers, peer_id, port, ip_address) {
						log::info!(
							"Added reserved {} peer {} (Total Reserved: {})",
							CHAINFLIP_P2P_PROTOCOL_NAME,
							peer_id,
							reserved_peers.len()
						);
						Ok(200)
					} else {
						Err(anyhow::anyhow!(
							"Tried to add peer {} which is already reserved",
							peer_id
						)
						.into())
					}
				}

				/// Disconnect from an authority
				fn remove_peer(&self, peer_id: PeerIdTransferable) -> RpcResult<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut reserved_peers = self.shared_state.reserved_peers.lock().unwrap();
					if reserved_peers.remove(&peer_id).is_some() {
						self.p2p_network_service.remove_reserved_peer(peer_id);
						log::info!(
							"Removed reserved {} peer {} (Total Reserved: {})",
							CHAINFLIP_P2P_PROTOCOL_NAME,
							peer_id,
							reserved_peers.len()
						);
						Ok(200)
					} else {
						Err(anyhow::anyhow!(
							"Tried to remove peer {} which is not reserved",
							peer_id
						)
						.into())
					}
				}

				fn setup_ipc_connections(&self, server_name: String) -> RpcResult<u64> {
					let (incoming_ipc_sender, incoming_ipc_receiver) =
						ipc_channel::ipc::channel::<(PeerIdTransferable, Vec<u8>)>()
							.context("Failed to create incoming p2p message IPC channel")?;
					let (outgoing_ipc_sender, outgoing_ipc_receiver) =
						ipc_channel::ipc::channel::<(Vec<PeerIdTransferable>, Vec<u8>)>()
							.context("Failed to create outgoing p2p message IPC channel")?;

					ipc_channel::ipc::IpcSender::connect(server_name)
						.context("Failed to connect to oneshot IPC channel")?
						.send((outgoing_ipc_sender, incoming_ipc_receiver))
						.context("Failed to setup IPC channels")?;

					*self.shared_state.ipc_state.lock().unwrap() = Some({
						let (
							outgoing_receiver_shutdown_sender,
							mut outgoing_receiver_shutdown_receiver,
						) = oneshot::channel();

						let shared_state = self.shared_state.clone();
						self.message_sender_spawner.spawn("IPCReceiver", "IPC", async move {
							let mut outgoing_ipc_stream = new_ipc_stream(outgoing_ipc_receiver);

							loop {
								tokio::select! {
									ipc_msg = outgoing_ipc_stream.next() => {
										let (peers, message) = match ipc_msg {
											Some(ipc_msg) => ipc_msg,
											None => break
										};

										if let Err(error) = (|| -> anyhow::Result<()> {
											let peers = peers
												.into_iter()
												.map(PeerIdTransferable::try_into)
												.collect::<std::result::Result<BTreeSet<_>, _>>()?;

											let reserved_peers = shared_state.reserved_peers.lock().unwrap();
											if peers.iter().all(|peer| reserved_peers.contains_key(peer)) {
												for peer_id in peers {
													let (_, _, message_sender) =
														reserved_peers.get(&peer_id).unwrap();
													match message_sender.try_send(message.clone()) {
														Ok(_) => (),
														Err(TrySendError::Full(_)) => {
															log::warn!("Dropping message for peer {}", peer_id);
														},
														Err(_) => unreachable!(
															"Receiver isn't dropped until sender is dropped"
														),
													}
												}

												Ok(())
											} else {
												Err(anyhow!("Request to send message to unset peer."))
											}
										})() {
											log::warn!("Error sending outgoing p2p message: {error}");
											break
										}
									}
									_ = &mut outgoing_receiver_shutdown_receiver => {
										break
									}
								}
							}

							log::info!("IPC outgoing channel closed");
						});

						IpcState {
							incoming_ipc_sender,
							_outgoing_receiver_shutdown_sender: outgoing_receiver_shutdown_sender,
						}
					});

					Ok(200)
				}
			}

			Arc::new(RpcRequestHandler {
				message_sender_spawner: message_sender_spawner.clone(),
				shared_state: shared_state.clone(),
				p2p_network_service: p2p_network_service.clone(),
				retry_send_period,
			})
		},
		// P2P Event Handler
		{
			let (internal_incoming_sender, internal_incoming_receiver) = std::sync::mpsc::channel();

			message_sender_spawner.spawn_blocking("IPCSender", "IPC", {
				async move {
					while let Ok((peer_id, message)) = internal_incoming_receiver.recv() {
						if let Some(ipc_state) = &*shared_state.ipc_state.lock().unwrap() {
							let _result = ipc_state.incoming_ipc_sender.send((peer_id, message));
						}
					}
				}
			});

			let mut network_event_stream = p2p_network_service.event_stream();

			async move {
				let mut total_connected_peers: usize = 0;
				while let Some(event) = network_event_stream.next().await {
					match event {
						/* A peer has connected to us */
						Event::NotificationStreamOpened {
							remote,
							protocol,
							role: _,
							negotiated_fallback: _,
						} =>
							if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
								total_connected_peers += 1;
								log::info!(
									"Connected and established {} with peer: {} (Total Connected: {})",
									protocol,
									remote,
									total_connected_peers
								);
							},
						/* A peer has disconnected from us */
						Event::NotificationStreamClosed { remote, protocol } => {
							if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
								total_connected_peers -= 1;
								log::info!(
									"Disconnected and closed {} with peer: {} (Total Connected: {})",
									protocol,
									remote,
									total_connected_peers
								);
							}
						},
						/* Received p2p messages from a peer */
						Event::NotificationsReceived { remote, messages } => {
							let mut messages = messages
								.into_iter()
								.filter_map(|(protocol, data)| {
									if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
										Some(data)
									} else {
										None
									}
								})
								.peekable();
							if messages.peek().is_some() {
								for message in messages {
									internal_incoming_sender
										.send((From::from(&remote), message.into_iter().collect()))
										.unwrap();
								}
							}
						},
						_ => {},
					}
				}
			}
		},
	)
}

type OutgoingSender = ipc_channel::ipc::IpcSender<(Vec<PeerIdTransferable>, Vec<u8>)>;

pub async fn setup_ipc_connections<C: P2PValidatorNetworkNodeRpcApiClient + Sync>(
	client: &C,
) -> anyhow::Result<(OutgoingSender, impl futures::Stream<Item = (PeerIdTransferable, Vec<u8>)>)> {
	let (server, server_name) = ipc_channel::ipc::IpcOneShotServer::new()?;

	client.setup_ipc_connections(server_name).await?;

	let (ipc_outgoing_sender, ipc_incoming_receiver) = server.accept()?.1;

	Ok((ipc_outgoing_sender, new_ipc_stream(ipc_incoming_receiver)))
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use super::*;
	use cf_utilities::mockall_utilities::eq;
	use futures::Future;
	use jsonrpsee::{
		ws_client::{WsClient, WsClientBuilder},
		ws_server::WsServerBuilder,
	};
	use mockall::Sequence;
	use tokio::sync::{Mutex, MutexGuard};

	struct LockedMockPeerNetwork(Mutex<MockPeerNetwork>);
	impl LockedMockPeerNetwork {
		fn lock(&self) -> MutexGuard<MockPeerNetwork> {
			self.0.try_lock().unwrap()
		}
	}
	#[async_trait]
	impl PeerNetwork for LockedMockPeerNetwork {
		fn reserve_peer(&self, peer_id: PeerId, port: Port, address: Ipv6Addr) {
			self.lock().reserve_peer(peer_id, port, address)
		}

		fn remove_reserved_peer(&self, peer_id: PeerId) {
			self.lock().remove_reserved_peer(peer_id)
		}

		async fn try_send_notification(&self, target: PeerId, message: &[u8]) -> bool {
			self.lock().try_send_notification(target, message).await
		}

		fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
			self.lock().event_stream()
		}
	}

	async fn new_p2p_network_node_with_test_probes() -> (
		tokio::sync::mpsc::UnboundedSender<Event>,
		WsClient,
		Arc<SharedState>,
		Arc<LockedMockPeerNetwork>,
		sc_service::TaskManager,
	) {
		let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();

		let network_expectations =
			Arc::new(LockedMockPeerNetwork(Mutex::new(MockPeerNetwork::new())));
		network_expectations.lock().expect_event_stream().return_once(move || {
			Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(event_receiver))
		});

		let handle = tokio::runtime::Handle::current();
		let task_manager = sc_service::TaskManager::new(handle, None).unwrap();
		let message_sender_spawn_handle = task_manager.spawn_handle();

		let (rpc_request_handler, p2p_message_handler_future) = new_p2p_network_node(
			network_expectations.clone(),
			message_sender_spawn_handle,
			Duration::from_secs(0),
		);

		let shared_state = rpc_request_handler.shared_state.clone();

		let server = WsServerBuilder::default().build("127.0.0.1:0").await.unwrap();
		let addr = format!("ws://{}", server.local_addr().unwrap());
		server
			.start(P2PValidatorNetworkNodeRpcApiServer::into_rpc(rpc_request_handler))
			.unwrap();

		let client = WsClientBuilder::default().build(addr).await.unwrap();

		tokio::runtime::Handle::current().spawn(p2p_message_handler_future);

		network_expectations.lock().checkpoint();

		(event_sender, client, shared_state, network_expectations, task_manager)
	}

	async fn expect_reserve_peer_changes_during_closure<F: Future, C: FnOnce() -> F>(
		shared_state: Arc<SharedState>,
		network_expectations: Arc<LockedMockPeerNetwork>,
		replaces: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
		removes: Vec<PeerIdTransferable>,
		adds: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
		final_state: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
		c: C,
	) {
		network_expectations.lock().checkpoint();
		for (peer_id, port, ip_address) in replaces {
			let mut seq = Sequence::new();
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.lock()
				.expect_remove_reserved_peer()
				.with(eq(peer_id))
				.times(1)
				.in_sequence(&mut seq)
				.return_const(());
			network_expectations
				.lock()
				.expect_reserve_peer()
				.with(eq(peer_id), eq(port), eq(ip_address))
				.times(1)
				.in_sequence(&mut seq)
				.return_const(());
		}
		for peer_id in removes {
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.lock()
				.expect_remove_reserved_peer()
				.with(eq(peer_id))
				.times(1)
				.return_const(());
		}
		for (peer_id, port, ip_address) in adds {
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.lock()
				.expect_reserve_peer()
				.with(eq(peer_id), eq(port), eq(ip_address))
				.times(1)
				.return_const(());
		}
		c().await;
		network_expectations.lock().checkpoint();
		assert_eq!(
			shared_state
				.reserved_peers
				.try_lock()
				.unwrap()
				.iter()
				.map(|(peer_id, (port, ip_address, _))| (*peer_id, (*port, *ip_address)))
				.collect::<BTreeMap<_, _>>(),
			final_state
				.into_iter()
				.map(|(peer_id, port, ip_address)| (
					peer_id.try_into().unwrap(),
					(port, ip_address)
				))
				.collect()
		);
	}

	#[tokio::test]
	async fn add_and_remove_peers() {
		let (_event_sender, client, shared_state, network_expectations, _task_manager) =
			new_p2p_network_node_with_test_probes().await;

		let client = Arc::new(client);

		let peer_0 = PeerIdTransferable::from(&PeerId::random());
		let peer_1 = PeerIdTransferable::from(&PeerId::random());

		let port_0: Port = 0;
		let port_1: Port = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let test_add_peer =
			|peer: PeerIdTransferable,
			 port: Port,
			 ip_address: std::net::Ipv6Addr,
			 replaces: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 peers: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let shared_state = shared_state.clone();
				expect_reserve_peer_changes_during_closure(
					shared_state,
					network_expectations,
					replaces,
					removes,
					adds,
					peers,
					move || async move {
						assert!(matches!(client.add_peer(peer, port, ip_address).await, Ok(_)));
					},
				)
			};

		let test_remove_peer =
			|peer: PeerIdTransferable,
			 replaces: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 peers: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let shared_state = shared_state.clone();
				expect_reserve_peer_changes_during_closure(
					shared_state,
					network_expectations,
					replaces,
					removes,
					adds,
					peers,
					move || async move {
						assert!(matches!(client.remove_peer(peer).await, Ok(_)));
					},
				)
			};

		// Tests

		assert!(matches!(
			client.remove_peer(PeerIdTransferable::from(&PeerId::random())).await,
			Err(_)
		));

		let peer_0_mapping = {
			// Added peers are reserved

			test_add_peer(
				peer_0.clone(),
				port_0,
				ip_address_0,
				vec![],
				vec![],
				vec![(peer_0.clone(), port_0, ip_address_0)],
				vec![(peer_0.clone(), port_0, ip_address_0)],
			)
			.await;

			// Repeat adds are rejected

			assert!(matches!(client.add_peer(peer_0.clone(), port_0, ip_address_0).await, Err(_)));

			// Peer mapping (ip address) update is allowed

			test_add_peer(
				peer_0.clone(),
				port_0,
				ip_address_1,
				vec![(peer_0.clone(), port_0, ip_address_1)],
				vec![],
				vec![],
				vec![(peer_0.clone(), port_0, ip_address_1)],
			)
			.await;

			// Peer mapping (port) update is allowed

			test_add_peer(
				peer_0.clone(),
				port_1,
				ip_address_0,
				vec![(peer_0.clone(), port_1, ip_address_0)],
				vec![],
				vec![],
				vec![(peer_0.clone(), port_1, ip_address_0)],
			)
			.await;

			// Peer mapping (ip address and port) update is allowed

			let expected_peer_mapping = (peer_0.clone(), port_0, ip_address_1);
			test_add_peer(
				peer_0.clone(),
				expected_peer_mapping.1,
				expected_peer_mapping.2,
				vec![expected_peer_mapping.clone()],
				vec![],
				vec![],
				vec![expected_peer_mapping.clone()],
			)
			.await;
			expected_peer_mapping
		};

		// Adding multiple peers

		test_add_peer(
			peer_1.clone(),
			port_0,
			ip_address_0,
			vec![],
			vec![],
			vec![(peer_1.clone(), port_0, ip_address_0)],
			vec![peer_0_mapping.clone(), (peer_1.clone(), port_0, ip_address_0)],
		)
		.await;

		// Removing peer preserves other peers

		test_remove_peer(
			peer_1.clone(),
			vec![],
			vec![peer_1.clone()],
			vec![],
			vec![peer_0_mapping.clone()],
		)
		.await;
	}

	#[tokio::test]
	async fn set_peers() {
		let (_event_sender, client, shared_state, network_expectations, _task_manager) =
			new_p2p_network_node_with_test_probes().await;

		let client = Arc::new(client);

		let peer_0 = PeerIdTransferable::from(&PeerId::random());
		let peer_1 = PeerIdTransferable::from(&PeerId::random());
		let peer_2 = PeerIdTransferable::from(&PeerId::random());

		let port_0: Port = 0;
		let port_1: Port = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let test_set_peers =
			|peers: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 replaces: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, Port, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let shared_state = shared_state.clone();
				expect_reserve_peer_changes_during_closure(
					shared_state,
					network_expectations,
					replaces,
					removes,
					adds,
					peers.clone(),
					move || async move {
						assert!(matches!(client.set_peers(peers.to_vec()).await, Ok(_)));
					},
				)
			};

		// Tests

		// Reject Invalid PeerIds

		assert!(matches!(
			client
				.set_peers(vec![(PeerIdTransferable(vec![3, 4]), port_1, ip_address_1)])
				.await,
			Err(_)
		));

		// Set 2 Valid Peer Ids

		test_set_peers(
			vec![(peer_0.clone(), port_0, ip_address_0), (peer_1.clone(), port_0, ip_address_0)],
			vec![],
			vec![],
			vec![(peer_0.clone(), port_0, ip_address_0), (peer_1.clone(), port_0, ip_address_0)],
		)
		.await;

		// Only reserve new Peer Ids

		test_set_peers(
			vec![
				(peer_0.clone(), port_0, ip_address_0),
				(peer_1.clone(), port_0, ip_address_0),
				(peer_2.clone(), port_0, ip_address_0),
			],
			vec![],
			vec![],
			vec![(peer_2.clone(), port_0, ip_address_0)],
		)
		.await;

		// Remove and Add Peers with different port/ip_address

		test_set_peers(
			vec![
				(peer_0.clone(), port_1, ip_address_1),
				(peer_1.clone(), port_0, ip_address_0),
				(peer_2.clone(), port_0, ip_address_0),
			],
			vec![(peer_0.clone(), port_1, ip_address_1)],
			vec![],
			vec![],
		)
		.await;

		// Remove peers from previous sets

		test_set_peers(
			vec![(peer_0.clone(), port_0, ip_address_1), (peer_2.clone(), port_0, ip_address_0)],
			vec![(peer_0.clone(), port_0, ip_address_1)],
			vec![peer_1.clone()],
			vec![],
		)
		.await;
	}

	#[tokio::test]
	async fn send_message() {
		let (_event_sender, client, _internal_state, network_expectations, _task_manager) =
			new_p2p_network_node_with_test_probes().await;

		let peer_0 = PeerId::random();
		let peer_1 = PeerId::random();
		let peer_2 = PeerId::random();

		let peer_0_transferable = PeerIdTransferable::from(&peer_0);
		let peer_1_transferable = PeerIdTransferable::from(&peer_1);
		let peer_2_transferable = PeerIdTransferable::from(&peer_2);

		let port_0: Port = 0;
		let port_1: Port = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let message = vec![4, 5, 6, 7, 8];

		let (ipc_outgoing_sender, _ipc_incoming_stream) =
			setup_ipc_connections(&client).await.unwrap();

		network_expectations.lock().expect_reserve_peer().times(2).return_const(());
		client
			.add_peer(peer_0_transferable.clone(), port_0, ip_address_0)
			.await
			.unwrap();
		client
			.add_peer(peer_1_transferable.clone(), port_1, ip_address_1)
			.await
			.unwrap();
		network_expectations.lock().checkpoint();

		// Tests

		// All peers get sent message

		network_expectations
			.lock()
			.expect_try_send_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(1)
			.return_const(true);
		network_expectations
			.lock()
			.expect_try_send_notification()
			.with(eq(peer_1), eq(message.clone()))
			.times(1)
			.return_const(true);

		ipc_outgoing_sender
			.send((vec![peer_0_transferable.clone(), peer_1_transferable.clone()], message.clone()))
			.unwrap();

		tokio::time::sleep(Duration::from_millis(50)).await; // See below

		// Peer gets sent message

		network_expectations
			.lock()
			.expect_try_send_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(1)
			.return_const(true);
		ipc_outgoing_sender
			.send((vec![peer_0_transferable.clone()], message.clone()))
			.unwrap();

		tokio::time::sleep(Duration::from_millis(50)).await; // See below

		// Retry failed message sends

		network_expectations
			.lock()
			.expect_try_send_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(RETRY_SEND_ATTEMPTS - 1)
			.return_const(false);
		network_expectations
			.lock()
			.expect_try_send_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(1)
			.return_const(true);
		ipc_outgoing_sender
			.send((vec![peer_0_transferable.clone()], message.clone()))
			.unwrap();

		tokio::time::sleep(Duration::from_millis(50)).await; // See below

		// Partially unreserved peers cause message to be not be sent

		ipc_outgoing_sender
			.send((vec![peer_0_transferable.clone(), peer_2_transferable.clone()], message.clone()))
			.unwrap();

		// Unreserved peer cause message to be not be sent

		ipc_outgoing_sender
			.send((vec![peer_2_transferable.clone()], message.clone()))
			.unwrap();

		// Need to make sure async spawned senders finish sending before checking expectations, we
		// currently don't have a better method
		tokio::time::sleep(Duration::from_millis(50)).await;
	}

	#[tokio::test]
	async fn incoming_messages_are_forwarded() {
		let (event_sender, client, _internal_state, _expectations, _task_manager) =
			new_p2p_network_node_with_test_probes().await;

		let peer_0 = PeerId::random();
		let peer_0_transferable = PeerIdTransferable::from(&peer_0);

		let message = vec![4, 5, 6, 7, 8];
		let other_message = vec![2, 3, 4, 5, 6];

		let (_ipc_outgoing_sender, mut ipc_incoming_stream) =
			setup_ipc_connections(&client).await.unwrap();

		// Tests

		// Only chainflip protocol messages are forwarded

		event_sender
			.send(Event::NotificationsReceived {
				remote: peer_0,
				messages: vec![(
					Cow::Borrowed("Not chainflip protocol"),
					other_message.clone().into(),
				)],
			})
			.unwrap();
		event_sender
			.send(Event::NotificationsReceived {
				remote: peer_0,
				messages: vec![
					(CHAINFLIP_P2P_PROTOCOL_NAME, message.clone().into()),
					(Cow::Borrowed("Not chainflip protocol 2"), other_message.clone().into()),
				],
			})
			.unwrap();

		assert_eq!(
			ipc_incoming_stream.next().await,
			Some((peer_0_transferable.clone(), message.clone()))
		);

		assert!(matches!(
			tokio::time::timeout(Duration::from_millis(20), ipc_incoming_stream.next()).await,
			Err(_)
		))
	}
}
