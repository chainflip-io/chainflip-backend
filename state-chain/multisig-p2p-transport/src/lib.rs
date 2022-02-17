//! Chainflip P2P layer.
//!
//! This code allows this node's CFE to communicate with other node's CFEs using substrate's
//! existing p2p network. We give substrate a RpcRequestHandler object which substrate uses to
//! process Rpc requests, and we create and run a background future that processes incoming p2p
//! messages and sends them to any Rpc subscribers we have (Our local CFE).

pub use gen_client::Client as P2PRpcClient;
pub use sc_network::PeerId;

use futures::{
	channel::mpsc::{unbounded, UnboundedSender},
	task::Spawn,
	FutureExt, SinkExt, StreamExt,
};
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{manager::SubscriptionManager, typed::Subscriber, SubscriptionId};
use sc_network::{multiaddr, Event, ExHashT, NetworkService};
use serde::{self, Deserialize, Serialize};
use sp_runtime::{
	sp_std::sync::{Arc, RwLock},
	traits::Block as BlockT,
};
use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet, HashMap},
	convert::TryInto,
	marker::Send,
	net::Ipv6Addr,
	pin::Pin,
};

#[cfg(test)]
use mockall::automock;

/// The identifier for our protocol, required to distinguish it from other protocols running on the
/// substrate p2p network.
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
		fallback_names: Vec::new(),
	}
}

/// A struct to encode a PeerId so it is Serializable and Deserializable
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PeerIdTransferable(Vec<u8>);

impl TryInto<PeerId> for PeerIdTransferable {
	type Error = jsonrpc_core::Error;

	fn try_into(self) -> std::result::Result<PeerId, Self::Error> {
		PeerId::from_bytes(&self.0[..])
			.map_err(|err| jsonrpc_core::Error::invalid_params(format!("{}", err)))
	}
}

impl From<&PeerId> for PeerIdTransferable {
	fn from(peer_id: &PeerId) -> Self {
		Self(peer_id.to_bytes())
	}
}

pub struct RpcRequestHandler<MetaData, P2PNetworkService: PeerNetwork> {
	/// Runs concurrently in the background and manages receiving (from the senders in
	/// "p2p_message_rpc_subscribers") and then actually sending P2PEvents to the Rpc subscribers
	notification_rpc_subscription_manager: SubscriptionManager,
	state: Arc<RwLock<P2PValidatorNetworkNodeState>>,
	p2p_network_service: Arc<P2PNetworkService>,
	_phantom: std::marker::PhantomData<MetaData>,
}

/// Shared state to allow Rpc to send P2P Messages, and the P2P to send Rpc notifcations
#[derive(Default)]
struct P2PValidatorNetworkNodeState {
	/// Store all local rpc subscriber senders
	p2p_message_rpc_subscribers:
		HashMap<SubscriptionId, UnboundedSender<(PeerIdTransferable, Vec<u8>)>>,
	reserved_peers: BTreeMap<PeerId, (u16, Ipv6Addr)>,
}

/// An abstration of the underlying network of peers.
#[cfg_attr(test, automock)]
pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peer(&self, peer_id: PeerId, port: u16, address: Ipv6Addr);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peer(&self, peer_id: PeerId);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, message: Vec<u8>);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>>;
}

/// An implementation of [PeerNetwork] using substrate's libp2p-based `NetworkService`.
impl<B: BlockT, H: ExHashT> PeerNetwork for NetworkService<B, H> {
	fn reserve_peer(&self, peer_id: PeerId, port: u16, address: Ipv6Addr) {
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
		if let Err(err) = self.remove_peers_from_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			std::iter::once(
				[multiaddr::Protocol::P2p(peer_id.into())]
					.iter()
					.cloned()
					.collect::<multiaddr::Multiaddr>(),
			)
			.collect(),
		) {
			log::error!(target: "p2p", "remove_peers_from_reserved_set failed: {}", err);
		}
	}

	fn write_notification(&self, target: PeerId, message: Vec<u8>) {
		self.write_notification(target, CHAINFLIP_P2P_PROTOCOL_NAME, message);
	}

	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
		Box::pin(self.event_stream("network-chainflip"))
	}
}

#[rpc]
pub trait P2PValidatorNetworkNodeRpcApi {
	/// RPC Metadata
	type Metadata;

	/// Connect to validators and disconnect from old validators
	#[rpc(name = "p2p_set_peers")]
	fn set_peers(&self, peers: Vec<(PeerIdTransferable, u16, Ipv6Addr)>) -> Result<u64>;

	/// Connect to a validator
	#[rpc(name = "p2p_add_peer")]
	fn add_peer(&self, peer_id: PeerIdTransferable, port: u16, address: Ipv6Addr) -> Result<u64>;

	/// Disconnect from a validator
	#[rpc(name = "p2p_remove_peer")]
	fn remove_peer(&self, peer_id: PeerIdTransferable) -> Result<u64>;

	/// Send a message to validators returning a HTTP status code
	#[rpc(name = "p2p_send_message")]
	fn send_message(&self, peer_ids: Vec<PeerIdTransferable>, message: Vec<u8>) -> Result<u64>;

	/// Subscribe to receive p2p messages
	#[pubsub(subscription = "cf_p2p_messages", subscribe, name = "cf_p2p_subscribeMessages")]
	fn subscribe_messages(
		&self,
		metadata: Self::Metadata,
		subscriber: Subscriber<(PeerIdTransferable, Vec<u8>)>,
	);

	/// Unsubscribe from receiving p2p messages
	#[pubsub(subscription = "cf_p2p_messages", unsubscribe, name = "cf_p2p_unsubscribeMessages")]
	fn unsubscribe_messages(
		&self,
		metadata: Option<Self::Metadata>,
		id: SubscriptionId,
	) -> Result<bool>;
}

pub fn new_p2p_validator_network_node<
	MetaData: jsonrpc_pubsub::PubSubMetadata + Send + Sync + 'static,
	PN: PeerNetwork + Send + Sync + 'static,
>(
	p2p_network_service: Arc<PN>,
	subscription_task_executor: impl Spawn + Send + Sync + 'static,
) -> (RpcRequestHandler<MetaData, PN>, impl futures::Future<Output = ()>) {
	let state = Arc::new(RwLock::new(P2PValidatorNetworkNodeState::default()));

	(
		// RPC Request Handler
		{
			impl<
					MetaData: jsonrpc_pubsub::PubSubMetadata + Send + Sync + 'static,
					PN: PeerNetwork + Send + Sync + 'static,
				> P2PValidatorNetworkNodeRpcApi for RpcRequestHandler<MetaData, PN>
			{
				type Metadata = MetaData;

				/// Connect to validators
				fn set_peers(
					&self,
					peers: Vec<(PeerIdTransferable, u16, Ipv6Addr)>,
				) -> Result<u64> {
					let mut peers = peers
						.into_iter()
						.map(|(peer_id, port, address)| {
							Result::Ok((peer_id.try_into()?, (port, address)))
						})
						.collect::<std::result::Result<BTreeMap<_, _>, _>>()?;

					let mut state = self.state.write().unwrap();
					std::mem::swap(&mut state.reserved_peers, &mut peers);

					// TODO: Investigate why adding/removing multiple reserved peers in a single
					// reserve_peers call doesn't work

					// TODO Check that removing then adding a peer is enough

					for (peer_id, _) in peers.iter().filter(|(peer_id, port_addr)| {
						state.reserved_peers.get(peer_id) != Some(port_addr)
					}) {
						self.p2p_network_service.remove_reserved_peer(*peer_id);
					}

					for (peer_id, (port, addr)) in state
						.reserved_peers
						.iter()
						.filter(|(peer_id, addr_port)| peers.get(peer_id) != Some(addr_port))
					{
						self.p2p_network_service.reserve_peer(*peer_id, *port, *addr);
					}

					log::info!(
						"Set {} reserved peers (Total Reserved: {})",
						CHAINFLIP_P2P_PROTOCOL_NAME,
						state.reserved_peers.len()
					);

					Ok(200)
				}

				/// Connect to a validator
				fn add_peer(
					&self,
					peer_id: PeerIdTransferable,
					port: u16,
					ip_address: Ipv6Addr,
				) -> Result<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut state = self.state.write().unwrap();
					if let Some(port_addr) = state.reserved_peers.get(&peer_id) {
						if port_addr == &(port, ip_address) {
							return Err(jsonrpc_core::Error::invalid_params(format!(
								"Tried to add peer {} which is already reserved",
								peer_id
							)))
						} else {
							self.p2p_network_service.remove_reserved_peer(peer_id);
						}
					}
					state.reserved_peers.insert(peer_id, (port, ip_address));
					self.p2p_network_service.reserve_peer(peer_id, port, ip_address);
					log::info!(
						"Added reserved {} peer (Total Reserved: {})",
						CHAINFLIP_P2P_PROTOCOL_NAME,
						state.reserved_peers.len()
					);
					Ok(200)
				}

				/// Disconnect from a validator
				fn remove_peer(&self, peer_id: PeerIdTransferable) -> Result<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut state = self.state.write().unwrap();
					if state.reserved_peers.remove(&peer_id).is_some() {
						self.p2p_network_service.remove_reserved_peer(peer_id);
						log::info!(
							"Removed reserved {} peer (Total Reserved: {})",
							CHAINFLIP_P2P_PROTOCOL_NAME,
							state.reserved_peers.len()
						);
						Ok(200)
					} else {
						Err(jsonrpc_core::Error::invalid_params(format!(
							"Tried to remove peer {} which is not reserved",
							peer_id
						)))
					}
				}

				/// Send message to peer
				fn send_message(
					&self,
					peers: Vec<PeerIdTransferable>,
					message: Vec<u8>,
				) -> Result<u64> {
					let peers = peers
						.into_iter()
						.map(PeerIdTransferable::try_into)
						.collect::<std::result::Result<BTreeSet<_>, _>>()?;

					let state = self.state.read().unwrap();
					if peers.iter().all(|peer| state.reserved_peers.contains_key(peer)) {
						for peer in peers {
							self.p2p_network_service.write_notification(peer, message.clone());
						}
						Ok(200)
					} else {
						Err(jsonrpc_core::Error::invalid_params(
							"Request to send message to an unset peer",
						))
					}
				}

				/// Subscribe to receive P2PEvents
				fn subscribe_messages(
					&self,
					_metadata: Self::Metadata,
					subscriber: Subscriber<(PeerIdTransferable, Vec<u8>)>,
				) {
					let (sender, receiver) = unbounded();
					let subscription_id = self.notification_rpc_subscription_manager.add(
						subscriber,
						move |sink| async move {
							sink.sink_map_err(|e| {
								log::warn!("Error sending notifications: {:?}", e)
							})
							.send_all(
								&mut receiver.map(Ok::<_, jsonrpc_core::Error>).map(Ok::<_, ()>),
							)
							.map(|_| ())
							.await
						},
					);
					self.state
						.write()
						.unwrap()
						.p2p_message_rpc_subscribers
						.insert(subscription_id, sender);
				}

				/// Unsubscribe to stop receiving P2PEvents
				fn unsubscribe_messages(
					&self,
					_metadata: Option<Self::Metadata>,
					id: SubscriptionId,
				) -> jsonrpc_core::Result<bool> {
					Ok(if self.notification_rpc_subscription_manager.cancel(id.clone()) {
						self.state
							.write()
							.unwrap()
							.p2p_message_rpc_subscribers
							.remove(&id)
							.unwrap();
						true
					} else {
						assert!(!self
							.state
							.read()
							.unwrap()
							.p2p_message_rpc_subscribers
							.contains_key(&id));
						false
					})
				}
			}

			RpcRequestHandler {
				state: state.clone(),
				p2p_network_service: p2p_network_service.clone(),
				notification_rpc_subscription_manager: SubscriptionManager::new(Arc::new(
					subscription_task_executor,
				)),
				_phantom: std::marker::PhantomData::<MetaData>::default(),
			}
		},
		// P2P Event Handler
		{
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
								let state = state.read().unwrap();
								let remote: PeerIdTransferable = From::from(&remote);
								for message in messages {
									let message = message.into_iter().collect::<Vec<u8>>();
									for sender in state.p2p_message_rpc_subscribers.values() {
										if let Err(e) =
											sender.unbounded_send((remote.clone(), message.clone()))
										{
											log::error!(
												"Failed to forward message to rpc subscriber: {}",
												e
											);
										}
									}
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

#[cfg(test)]
mod tests {
	use std::{
		sync::{RwLockReadGuard, RwLockWriteGuard},
		time::Duration,
	};

	use super::*;
	use futures::Future;
	use jsonrpc_core::MetaIoHandler;
	use jsonrpc_core_client::transports::local;
	use mockall::{predicate::eq, Sequence};

	struct LockedMockPeerNetwork(RwLock<MockPeerNetwork>);
	impl LockedMockPeerNetwork {
		fn read(&self) -> RwLockReadGuard<MockPeerNetwork> {
			self.0.read().unwrap()
		}
		fn write(&self) -> RwLockWriteGuard<MockPeerNetwork> {
			self.0.write().unwrap()
		}
	}
	impl PeerNetwork for LockedMockPeerNetwork {
		fn reserve_peer(&self, peer_id: PeerId, port: u16, address: Ipv6Addr) {
			self.read().reserve_peer(peer_id, port, address)
		}

		fn remove_reserved_peer(&self, peer_id: PeerId) {
			self.read().remove_reserved_peer(peer_id)
		}

		fn write_notification(&self, target: PeerId, message: Vec<u8>) {
			self.read().write_notification(target, message)
		}

		fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
			self.read().event_stream()
		}
	}

	async fn new_p2p_validator_network_node_with_test_probes() -> (
		tokio::sync::mpsc::UnboundedSender<Event>,
		P2PRpcClient,
		Arc<RwLock<P2PValidatorNetworkNodeState>>,
		Arc<LockedMockPeerNetwork>,
	) {
		let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();

		let network_expectations =
			Arc::new(LockedMockPeerNetwork(RwLock::new(MockPeerNetwork::new())));
		network_expectations.write().expect_event_stream().return_once(move || {
			Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(event_receiver))
		});

		let (rpc_request_handler, p2p_message_handler_future) = new_p2p_validator_network_node(
			network_expectations.clone(),
			sc_rpc::testing::TaskExecutor,
		);

		let internal_state = rpc_request_handler.state.clone();

		let (client, server) = local::connect_with_pubsub::<P2PRpcClient, _>(Arc::new({
			let mut io = MetaIoHandler::default();
			io.extend_with(P2PValidatorNetworkNodeRpcApi::to_delegate(rpc_request_handler));
			io
		}));

		tokio::runtime::Handle::current().spawn(server);
		tokio::runtime::Handle::current().spawn(p2p_message_handler_future);

		network_expectations.write().checkpoint();

		(event_sender, client, internal_state, network_expectations)
	}

	async fn expect_reserve_peer_changes_during_closure<F: Future, C: FnOnce() -> F>(
		internal_state: Arc<RwLock<P2PValidatorNetworkNodeState>>,
		network_expectations: Arc<LockedMockPeerNetwork>,
		replaces: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
		removes: Vec<PeerIdTransferable>,
		adds: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
		final_state: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
		c: C,
	) {
		network_expectations.write().checkpoint();
		for (peer_id, port, ip_address) in replaces {
			let mut seq = Sequence::new();
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.write()
				.expect_remove_reserved_peer()
				.with(eq(peer_id))
				.times(1)
				.in_sequence(&mut seq)
				.return_const(());
			network_expectations
				.write()
				.expect_reserve_peer()
				.with(eq(peer_id), eq(port), eq(ip_address))
				.times(1)
				.in_sequence(&mut seq)
				.return_const(());
		}
		for peer_id in removes {
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.write()
				.expect_remove_reserved_peer()
				.with(eq(peer_id))
				.times(1)
				.return_const(());
		}
		for (peer_id, port, ip_address) in adds {
			let peer_id: PeerId = peer_id.try_into().unwrap();
			network_expectations
				.write()
				.expect_reserve_peer()
				.with(eq(peer_id), eq(port), eq(ip_address))
				.times(1)
				.return_const(());
		}
		c().await;
		network_expectations.write().checkpoint();
		assert_eq!(
			internal_state.read().unwrap().reserved_peers,
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
		let (_event_sender, client, internal_state, network_expectations) =
			new_p2p_validator_network_node_with_test_probes().await;

		let peer_0 = PeerIdTransferable::from(&PeerId::random());
		let peer_1 = PeerIdTransferable::from(&PeerId::random());

		let port_0: u16 = 0;
		let port_1: u16 = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let test_add_peer =
			|peer: PeerIdTransferable,
			 port: u16,
			 ip_address: std::net::Ipv6Addr,
			 replaces: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 peers: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let internal_state = internal_state.clone();
				expect_reserve_peer_changes_during_closure(
					internal_state,
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
			 replaces: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 peers: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let internal_state = internal_state.clone();
				expect_reserve_peer_changes_during_closure(
					internal_state,
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
		let (_event_sender, client, internal_state, network_expectations) =
			new_p2p_validator_network_node_with_test_probes().await;

		let peer_0 = PeerIdTransferable::from(&PeerId::random());
		let peer_1 = PeerIdTransferable::from(&PeerId::random());
		let peer_2 = PeerIdTransferable::from(&PeerId::random());

		let port_0: u16 = 0;
		let port_1: u16 = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let test_set_peers =
			|peers: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 replaces: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>,
			 removes: Vec<PeerIdTransferable>,
			 adds: Vec<(PeerIdTransferable, u16, std::net::Ipv6Addr)>| {
				let network_expectations = network_expectations.clone();
				let client = client.clone();
				let internal_state = internal_state.clone();
				expect_reserve_peer_changes_during_closure(
					internal_state,
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
		let (_event_sender, client, _internal_state, network_expectations) =
			new_p2p_validator_network_node_with_test_probes().await;

		let peer_0 = PeerId::random();
		let peer_1 = PeerId::random();
		let peer_2 = PeerId::random();

		let peer_0_transferable = PeerIdTransferable::from(&peer_0);
		let peer_1_transferable = PeerIdTransferable::from(&peer_1);
		let peer_2_transferable = PeerIdTransferable::from(&peer_2);

		let port_0: u16 = 0;
		let port_1: u16 = 1;

		let ip_address_0: std::net::Ipv6Addr = 0.into();
		let ip_address_1: std::net::Ipv6Addr = 1.into();

		let message = vec![4, 5, 6, 7, 8];

		network_expectations.write().expect_reserve_peer().times(2).return_const(());
		client
			.add_peer(peer_0_transferable.clone(), port_0, ip_address_0)
			.await
			.unwrap();
		client
			.add_peer(peer_1_transferable.clone(), port_1, ip_address_1)
			.await
			.unwrap();
		network_expectations.write().checkpoint();

		// Tests

		// All peers get sent message

		network_expectations
			.write()
			.expect_write_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(1)
			.return_const(());
		network_expectations
			.write()
			.expect_write_notification()
			.with(eq(peer_1), eq(message.clone()))
			.times(1)
			.return_const(());
		assert!(matches!(
			client
				.send_message(
					vec![peer_0_transferable.clone(), peer_1_transferable.clone()],
					message.clone()
				)
				.await,
			Ok(_)
		));
		network_expectations.write().checkpoint();

		// Peer gets sent message

		network_expectations
			.write()
			.expect_write_notification()
			.with(eq(peer_0), eq(message.clone()))
			.times(1)
			.return_const(());
		assert!(matches!(
			client.send_message(vec![peer_0_transferable.clone()], message.clone()).await,
			Ok(_)
		));
		network_expectations.write().checkpoint();

		// Partially unreserved peers cause message to be not be sent

		assert!(matches!(
			client
				.send_message(
					vec![peer_0_transferable.clone(), peer_2_transferable.clone()],
					message.clone()
				)
				.await,
			Err(_)
		));

		// Unreserved peer cause message to be not be sent

		assert!(matches!(
			client.send_message(vec![peer_2_transferable.clone()], message.clone()).await,
			Err(_)
		));
	}

	#[tokio::test]
	async fn rpc_subscribe() {
		let (event_sender, client, _internal_state, _expectations) =
			new_p2p_validator_network_node_with_test_probes().await;

		let peer_0 = PeerId::random();
		let peer_0_transferable = PeerIdTransferable::from(&peer_0);

		let message = vec![4, 5, 6, 7, 8];
		let other_message = vec![2, 3, 4, 5, 6];

		let mut message_stream = client.subscribe_messages().unwrap();

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
			message_stream.next().await.unwrap().unwrap(),
			(peer_0_transferable.clone(), message.clone())
		);

		assert!(matches!(
			tokio::time::timeout(Duration::from_millis(20), message_stream.next()).await,
			Err(_)
		));
	}
}
