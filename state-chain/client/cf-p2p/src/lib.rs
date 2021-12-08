//! Chainflip P2P layer.
//!
//! This code allows this node's CFE to communicate with other node's CFEs using substrate's
//! existing p2p network. We give substrate a RpcRequestHandler object which substrate uses to
//! process Rpc requests, and we create and run a background future that processes incoming p2p
//! messages and sends them to any Rpc subscribers we have (Our local CFE).

pub use gen_client::Client as P2PRpcClient;
pub use sc_network::PeerId;

use core::iter;
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
	collections::{BTreeSet, HashMap},
	convert::TryInto,
	marker::Send,
	pin::Pin,
};

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
	reserved_peers: BTreeSet<PeerId>,
}

/// An abstration of the underlying network of peers.
pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peers<Peers: Iterator<Item = PeerId>>(&self, peers: Peers);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peers<Peers: Iterator<Item = PeerId>>(&self, peers: Peers);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, message: Vec<u8>);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>>;
}

/// An implementation of [PeerNetwork] using substrate's libp2p-based `NetworkService`.
impl<B: BlockT, H: ExHashT> PeerNetwork for NetworkService<B, H> {
	fn reserve_peers<Peers: Iterator<Item = PeerId>>(&self, peers: Peers) {
		if let Err(err) = self.add_peers_to_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			peers
				.map(|peer| {
					iter::once(multiaddr::Protocol::P2p(peer.into()))
						.collect::<multiaddr::Multiaddr>()
				})
				.collect(),
		) {
			log::error!(target: "p2p", "add_peers_to_reserved_set failed: {}", err);
		}
	}

	fn remove_reserved_peers<Peers: Iterator<Item = PeerId>>(&self, peers: Peers) {
		if let Err(err) = self.remove_peers_from_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			peers
				.map(|peer| {
					iter::once(multiaddr::Protocol::P2p(peer.into()))
						.collect::<multiaddr::Multiaddr>()
				})
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
	fn set_peers(&self, peer_ids: Vec<PeerIdTransferable>) -> Result<u64>;

	/// Connect to a validator
	#[rpc(name = "p2p_add_peer")]
	fn add_peer(&self, peer_id: PeerIdTransferable) -> Result<u64>;

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
				fn set_peers(&self, peers: Vec<PeerIdTransferable>) -> Result<u64> {
					let mut peers = peers
						.into_iter()
						.map(PeerIdTransferable::try_into)
						.collect::<std::result::Result<BTreeSet<_>, _>>()?;

					let mut state = self.state.write().unwrap();
					std::mem::swap(&mut state.reserved_peers, &mut peers);

					// TODO: Investigate why adding multiple reserved peers in a single
					// reserve_peers call doesn't work
					for peer in state.reserved_peers.difference(&peers).into_iter().cloned() {
						self.p2p_network_service.reserve_peers(std::iter::once(peer));
					}
					for peer in peers.difference(&state.reserved_peers).into_iter().cloned() {
						self.p2p_network_service.remove_reserved_peers(std::iter::once(peer));
					}

					Ok(200)
				}

				/// Connect to a validator
				fn add_peer(&self, peer_id: PeerIdTransferable) -> Result<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut state = self.state.write().unwrap();
					if state.reserved_peers.insert(peer_id.clone()) {
						self.p2p_network_service.reserve_peers(std::iter::once(peer_id));
						Ok(200)
					} else {
						Err(jsonrpc_core::Error::invalid_params(format!(
							"Tried to add peer {} which is already reserved",
							peer_id
						)))
					}
				}

				/// Disconnect from a validator
				fn remove_peer(&self, peer_id: PeerIdTransferable) -> Result<u64> {
					let peer_id: PeerId = peer_id.try_into()?;
					let mut state = self.state.write().unwrap();
					if state.reserved_peers.remove(&peer_id) {
						self.p2p_network_service.remove_reserved_peers(std::iter::once(peer_id));
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
					if peers.iter().all(|peer| state.reserved_peers.contains(peer)) {
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
								log::info!(
									"Connected and established {} with peer: {}",
									protocol,
									remote
								);
							},
						/* A peer has disconnected from us */
						Event::NotificationStreamClosed { remote, protocol } => {
							if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
								log::info!(
									"Disconnected and closed {} with peer: {}",
									protocol,
									remote
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

/*
#[cfg(test)]
mod tests {

	use super::*;
	use jsonrpc_core::MetaIoHandler;
	use jsonrpc_core_client::{transports::local, RpcError, TypedSubscriptionStream};
	use tokio;
	use tokio_stream::wrappers::UnboundedReceiverStream;

	struct TestNetwork {
		validators: Mutex<HashMap<PeerId, tokio::sync::mpsc::UnboundedSender<Event>>>,
	}
	impl TestNetwork {
		fn new() -> Arc<Self> {
			Arc::new(Self { validators: Default::default() })
		}
	}

	struct TestNetworkInterface {
		peer_id: PeerId,
		network: Arc<TestNetwork>,
	}
	impl TestNetworkInterface {
		fn new(peer_id: PeerId, network: Arc<TestNetwork>) -> Self {
			Self { peer_id, network }
		}
	}
	impl Drop for TestNetworkInterface {
		fn drop(&mut self) {
			let mut validators = self.network.validators.lock().unwrap();
			validators.remove(&self.peer_id);
			for remote_sender in validators.values() {
				remote_sender
					.send(Event::NotificationStreamClosed {
						remote: self.peer_id,
						protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
					})
					.unwrap();
				remote_sender.send(Event::SyncDisconnected { remote: self.peer_id }).unwrap();
			}
		}
	}
	impl PeerNetwork for TestNetworkInterface {
		fn reserve_peers<Peers : Iterator<Item=PeerId>>(&self, peers: Peers) {}

		fn remove_reserved_peers<Peers : Iterator<Item=PeerId>>(&self, peers: Peers) {}

		fn write_notification(&self, who: PeerId, message: Vec<u8>) {
			let validators = self.network.validators.lock().unwrap();
			if let Some(sender) = validators.get(&who) {
				sender
					.send(Event::NotificationsReceived {
						remote: self.peer_id,
						messages: vec![(CHAINFLIP_P2P_PROTOCOL_NAME, message.into())],
					})
					.unwrap();
			}
		}

		fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
			let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
			let mut validators = self.network.validators.lock().unwrap();
			for (remote_peer_id, remote_sender) in validators.iter() {
				use sc_network::ObservedRole;

				remote_sender.send(Event::SyncConnected { remote: self.peer_id }).unwrap();
				remote_sender
					.send(Event::NotificationStreamOpened {
						remote: self.peer_id,
						protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
						role: ObservedRole::Full,
						negotiated_fallback: None,
					})
					.unwrap();
				sender.send(Event::SyncConnected { remote: *remote_peer_id }).unwrap();
				sender
					.send(Event::NotificationStreamOpened {
						remote: *remote_peer_id,
						protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
						role: ObservedRole::Full,
						negotiated_fallback: None,
					})
					.unwrap();
			}

			use std::collections::hash_map;
			match validators.entry(self.peer_id) {
				hash_map::Entry::Occupied(_entry) => Err(()),
				hash_map::Entry::Vacant(entry) => Ok(entry.insert(sender)),
			}
			.unwrap(); // Assumed to be called once
			Box::pin(UnboundedReceiverStream::new(receiver))
		}
	}

	fn new_node(peer_id: PeerId, network: Arc<TestNetwork>) -> P2PRpcClient {
		let (rpc_request_handler, p2p_event_handler_fut) = new_p2p_validator_network_node(
			Arc::new(TestNetworkInterface::new(peer_id, network)),
			sc_rpc::testing::TaskExecutor,
		);
		let (client, server) = local::connect_with_pubsub::<P2PRpcClient, _>(Arc::new({
			let mut io = MetaIoHandler::default();
			io.extend_with(P2PValidatorNetworkNodeRpcApi::to_delegate(rpc_request_handler));
			io
		}));

		tokio::runtime::Handle::current().spawn(server);
		tokio::runtime::Handle::current().spawn(p2p_event_handler_fut);

		client
	}

	#[tokio::test]
	async fn repeat_self_identify_doesnt_fail_unless_id_different() {
		let network = TestNetwork::new();
		let node_0 = new_node(PeerId::random(), network.clone());

		let try_self_identify =
			|account_id: [u8; 32]| node_0.self_identify(AccountIdBs58(account_id));

		let matching_id = [1; 32];
		assert!(matches!(try_self_identify(matching_id).await, Ok(200u64)));
		assert!(matches!(try_self_identify(matching_id).await, Ok(200u64)));
		assert!(matches!(try_self_identify([2; 32]).await, Err(RpcError::JsonRpcError(_))));
	}

	#[tokio::test]
	async fn send_without_self_identify_fails() {
		let network = TestNetwork::new();
		let node_0 = new_node(PeerId::random(), network.clone());
		let node_1 = new_node(PeerId::random(), network.clone());

		let mut node1_notification_stream = node_0.subscribe_notifications().unwrap();

		let node_1_account_id = AccountIdBs58([5; 32]);
		node_1.self_identify(node_1_account_id.clone()).await.unwrap();
		assert_eq!(
			node1_notification_stream.next().await.unwrap().unwrap(),
			P2PEvent::ValidatorConnected(node_1_account_id.clone())
		);

		let try_send = || async {
			node_0
				.send(node_1_account_id.clone(), MessageBs58(Vec::from(&b"hello"[..])))
				.await
		};

		assert!(matches!(try_send().await, Err(RpcError::JsonRpcError(_))));
		assert!(matches!(node_0.self_identify(AccountIdBs58([1; 32])).await, Ok(200u64)));
		assert!(matches!(try_send().await, Ok(200u64)));
	}

	#[tokio::test]
	async fn broadcast_without_self_identify_fails() {
		let network = TestNetwork::new();
		let node_0 = new_node(PeerId::random(), network.clone());

		let try_broadcast =
			|| async { node_0.broadcast(MessageBs58(Vec::from(&b"hello"[..]))).await };

		assert!(matches!(try_broadcast().await, Err(RpcError::JsonRpcError(_))));
		assert!(matches!(node_0.self_identify(AccountIdBs58([1; 32])).await, Ok(200u64)));
		assert!(matches!(try_broadcast().await, Ok(200u64)));
	}

	#[tokio::test]
	async fn subscribe_receives_notifications() {
		let network = TestNetwork::new();
		let node_0 = new_node(PeerId::random(), network.clone());
		let node_1 = new_node(PeerId::random(), network.clone());

		let mut node1_notification_stream = node_0.subscribe_notifications().unwrap();

		let account_id = AccountIdBs58([5; 32]);
		node_1.self_identify(account_id.clone()).await.unwrap();
		assert_eq!(
			node1_notification_stream.next().await.unwrap().unwrap(),
			P2PEvent::ValidatorConnected(account_id)
		);
	}

	async fn new_node_with_subscribe_and_self_identify_and_wait_for_peer_self_identifies<
		'a,
		Iter: Iterator<Item = &'a AccountIdBs58> + Clone,
	>(
		peer_id: PeerId,
		account_id: &AccountIdBs58,
		other_account_ids: Iter,
		network: Arc<TestNetwork>,
		subscribe_barrier: &tokio::sync::Barrier,
	) -> (P2PRpcClient, TypedSubscriptionStream<P2PEvent>) {
		let node = new_node(peer_id, network.clone());
		let mut stream = node.subscribe_notifications().unwrap();
		subscribe_barrier.wait().await;

		node.self_identify(account_id.clone()).await.unwrap();

		let mut messages = vec![];
		for _ in other_account_ids.clone() {
			messages.push(stream.next().await.unwrap().unwrap());
		}
		for other_account_id in other_account_ids {
			assert!(messages.contains(&P2PEvent::ValidatorConnected(other_account_id.clone())));
		}

		(node, stream)
	}

	fn no_more_messages(mut stream: TypedSubscriptionStream<P2PEvent>) {
		tokio::spawn(async move {
			while let Some(event) = stream.next().await {
				assert!(
					matches!(event, Ok(P2PEvent::ValidatorDisconnected(_))),
					"Received unexpected message"
				);
			}
		});
	}

	#[tokio::test]
	async fn send_and_broadcast_are_received() {
		let network = TestNetwork::new();
		let subscribe_barrier = Arc::new(tokio::sync::Barrier::new(3));

		let node_0_sent_message = MessageBs58(Vec::from(&b"hello"[..]));
		let node_2_broadcast_message = MessageBs58(Vec::from(&b"world"[..]));

		let node_0_account_id = AccountIdBs58([5; 32]);
		let node_1_account_id = AccountIdBs58([4; 32]);
		let node_2_account_id = AccountIdBs58([3; 32]);

		tokio::join!(
			// node_0
			async {
				let (node, mut stream) =
					new_node_with_subscribe_and_self_identify_and_wait_for_peer_self_identifies(
						PeerId::random(),
						&node_0_account_id,
						[node_1_account_id.clone(), node_2_account_id.clone()].iter(),
						network.clone(),
						&subscribe_barrier,
					)
					.await;

				assert!(matches!(
					node.send(node_1_account_id.clone(), node_0_sent_message.clone()).await,
					Ok(200u64)
				));

				assert_eq!(
					stream.next().await.unwrap().unwrap(),
					P2PEvent::MessageReceived(
						node_2_account_id.clone(),
						node_2_broadcast_message.clone()
					)
				);

				no_more_messages(stream);
			},
			// node_1
			async {
				let (_node, mut stream) =
					new_node_with_subscribe_and_self_identify_and_wait_for_peer_self_identifies(
						PeerId::random(),
						&node_1_account_id,
						[node_0_account_id.clone(), node_2_account_id.clone()].iter(),
						network.clone(),
						&subscribe_barrier,
					)
					.await;

				{
					let messages = vec![
						stream.next().await.unwrap().unwrap(),
						stream.next().await.unwrap().unwrap(),
					];
					assert!(messages.contains(&P2PEvent::MessageReceived(
						node_0_account_id.clone(),
						node_0_sent_message.clone()
					)));
					assert!(messages.contains(&P2PEvent::MessageReceived(
						node_2_account_id.clone(),
						node_2_broadcast_message.clone()
					)));
				}

				no_more_messages(stream);
			},
			// node_2
			async {
				let (node, stream) =
					new_node_with_subscribe_and_self_identify_and_wait_for_peer_self_identifies(
						PeerId::random(),
						&node_2_account_id,
						[node_0_account_id.clone(), node_1_account_id.clone()].iter(),
						network.clone(),
						&subscribe_barrier,
					)
					.await;

				assert!(matches!(
					node.broadcast(node_2_broadcast_message.clone()).await,
					Ok(200u64)
				));

				no_more_messages(stream);
			}
		);
	}
}
*/
