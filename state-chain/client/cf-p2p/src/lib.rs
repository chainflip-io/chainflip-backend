//! Chainflip P2P layer.
//!
//! This code allows this node's CFE to communicate with other node's CFEs using substrate's existing p2p network.
//! We give substrate a RpcRequestHandler object which substrate uses to process Rpc requests, and we create and run a
//! background future that processes incoming p2p messages and sends them to any Rpc subscribers we have (Our local CFE).

pub mod p2p_serde;
pub use gen_client::Client as P2PRpcClient;

use core::iter;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::{StreamExt, TryStreamExt};
use jsonrpc_core::futures::Sink;
use jsonrpc_core::futures::{future::Executor, Future, Stream};
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{manager::SubscriptionManager, typed::Subscriber, SubscriptionId};
use log::{debug, warn};
use sc_network::{multiaddr, Event, ExHashT, NetworkService, PeerId};
use serde::{self, Deserialize, Serialize};
use sp_runtime::sp_std::sync::{Arc, Mutex};
use sp_runtime::traits::Block as BlockT;
use std::borrow::Cow;
use std::collections::{hash_map::Entry, HashMap};
use std::marker::Send;
use std::pin::Pin;

// TODO: This is duplicated in the CFE, can we just use one of these?
/// The type of validator id expected by the p2p layer, uses standard serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub [u8; 32]);

/// A wrapper around a byte buffer containing some opaque message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMessage(pub Vec<u8>);

/// The protocol has two message types, `Identify` and `Message`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum P2PMessage {
	SelfIdentify(AccountId),
	Message(RawMessage),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountIdBs58(#[serde(with = "p2p_serde::bs58_fixed_size")] pub [u8; 32]);

impl From<AccountIdBs58> for AccountId {
	fn from(id: AccountIdBs58) -> Self {
		Self(id.0)
	}
}

impl From<AccountId> for AccountIdBs58 {
	fn from(id: AccountId) -> Self {
		Self(id.0)
	}
}

impl std::fmt::Display for AccountIdBs58 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", bs58::encode(&self.0).into_string())
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBs58(#[serde(with = "p2p_serde::bs58_vec")] pub Vec<u8>);

impl From<MessageBs58> for RawMessage {
	fn from(msg: MessageBs58) -> Self {
		Self(msg.0)
	}
}

impl From<RawMessage> for MessageBs58 {
	fn from(msg: RawMessage) -> Self {
		Self(msg.0)
	}
}

impl std::fmt::Display for MessageBs58 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", bs58::encode(&self.0).into_string())
	}
}

/// Events available via the subscription stream.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum P2PEvent {
	/// A message has been received from another validator.
	MessageReceived(AccountIdBs58, MessageBs58),
	/// A new validator has cconnected and identified itself to the network.
	ValidatorConnected(AccountIdBs58),
	/// A validator has disconnected from the network.
	ValidatorDisconnected(AccountIdBs58),
}

/// The identifier for our protocol, required to distinguish it from other protocols running on the substrate p2p
/// network.
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
	}
}

/// An abstration of the underlying network of peers.
pub trait PeerNetwork {
	/// Adds the peer to the set of peers to be connected to with this protocol.
	fn reserve_peer(&self, who: PeerId);
	/// Removes the peer from the set of peers to be connected to with this protocol.
	fn remove_reserved_peer(&self, who: PeerId);
	/// Write notification to network to peer id, over protocol
	fn write_notification(&self, who: PeerId, message: Vec<u8>);
	/// Network event stream
	fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>>;
}

/// An implementation of [PeerNetwork] using substrate's libp2p-based `NetworkService`.
impl<B: BlockT, H: ExHashT> PeerNetwork for NetworkService<B, H> {
	fn reserve_peer(&self, who: PeerId) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result =
			self.add_peers_to_reserved_set(CHAINFLIP_P2P_PROTOCOL_NAME, iter::once(addr).collect());
		if let Err(err) = result {
			log::error!(target: "p2p", "add_set_reserved failed: {}", err);
		}
	}

	fn remove_reserved_peer(&self, who: PeerId) {
		let addr =
			iter::once(multiaddr::Protocol::P2p(who.into())).collect::<multiaddr::Multiaddr>();
		let result = self.remove_peers_from_reserved_set(
			CHAINFLIP_P2P_PROTOCOL_NAME,
			iter::once(addr).collect(),
		);
		if let Err(err) = result {
			log::error!(target: "p2p", "remove_set_reserved failed: {}", err);
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

	/// Identify yourself to the network.
	#[rpc(name = "p2p_self_identify")]
	fn self_identify(&self, validator_id: AccountIdBs58) -> Result<u64>;

	/// Send a message to validator id returning a HTTP status code
	#[rpc(name = "p2p_send")]
	fn send(&self, validator_id: AccountIdBs58, message: MessageBs58) -> Result<u64>;

	/// Broadcast a message to the p2p network returning a HTTP status code
	#[rpc(name = "p2p_broadcast")]
	fn broadcast(&self, message: MessageBs58) -> Result<u64>;

	/// Subscribe to receive notifications
	#[pubsub(
		subscription = "cf_p2p_notifications",
		subscribe,
		name = "cf_p2p_subscribeNotifications"
	)]
	fn subscribe_notifications(&self, metadata: Self::Metadata, subscriber: Subscriber<P2PEvent>);

	/// Unsubscribe from receiving notifications
	#[pubsub(
		subscription = "cf_p2p_notifications",
		unsubscribe,
		name = "cf_p2p_unsubscribeNotifications"
	)]
	fn unsubscribe_notifications(
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
	subscription_task_executor: impl Executor<Box<(dyn Future<Item = (), Error = ()> + Send)>>
		+ Send
		+ Sync
		+ 'static,
) -> (
	jsonrpc_core::MetaIoHandler<MetaData>,
	impl futures::Future<Output = ()>,
) {
	/// Encodes the message using bincode and sends it over the p2p network
	fn encode_and_send<'a, Network: PeerNetwork, Peers: Iterator<Item = &'a PeerId>>(
		p2p_network_service: &Arc<Network>,
		message: P2PMessage,
		peers: Peers,
	) {
		match bincode::serialize(&message) {
			Ok(bytes) => {
				for peer in peers {
					p2p_network_service.write_notification(*peer, bytes.clone());
				}
			}
			Err(err) => {
				log::error!("Error while serializing p2p protocol message {}", err);
			}
		}
	}

	// Shared state to allow Rpc to send P2P Messages, and the P2P to send Rpc notifcations
	struct P2PValidatorNetworkNodeState {
		/// Store all local rpc subscriber senders
		notification_rpc_subscribers: HashMap<SubscriptionId, UnboundedSender<P2PEvent>>,
		/// PeerIds with the corresponding AccountId, if available.
		peer_to_validator: HashMap<PeerId, Option<AccountId>>,
		/// ValidatorIds mapped to corresponding PeerIds.
		validator_to_peer: HashMap<AccountId, PeerId>,
		/// Our own AccountId
		local_validator_id: Option<AccountId>,
	}
	let state = Arc::new(Mutex::new(P2PValidatorNetworkNodeState {
		notification_rpc_subscribers: Default::default(),
		peer_to_validator: Default::default(),
		validator_to_peer: Default::default(),
		local_validator_id: None,
	}));

	(
		// RPC Request Handler
		{
			struct RpcRequestHandler<MetaData, P2PNetworkService: PeerNetwork> {
				/// Runs concurrently in the background and manages receiving (from the senders in "notification_rpc_subscribers") and then actually sending P2PEvents to the Rpc subscribers
				notification_rpc_subscription_manager: SubscriptionManager,
				state: Arc<Mutex<P2PValidatorNetworkNodeState>>,
				p2p_network_service: Arc<P2PNetworkService>,
				_phantom: std::marker::PhantomData<MetaData>,
			}
			fn check_p2p_message_is_valid(
				state: &P2PValidatorNetworkNodeState,
				message: &MessageBs58,
			) -> Result<()> {
				if message.0.is_empty() {
					Err(jsonrpc_core::Error::invalid_params("Empty p2p message"))
				} else if state.local_validator_id.is_none() {
					Err(jsonrpc_core::Error::invalid_params(
						"Cannot send p2p message before self identification",
					))
				} else {
					Ok(())
				}
			}
			impl<
					MetaData: jsonrpc_pubsub::PubSubMetadata + Send + Sync + 'static,
					PN: PeerNetwork + Send + Sync + 'static,
				> P2PValidatorNetworkNodeRpcApi for RpcRequestHandler<MetaData, PN>
			{
				type Metadata = MetaData;

				/// Identify ourselves to the network
				fn self_identify(&self, validator_id: AccountIdBs58) -> Result<u64> {
					let mut state = self.state.lock().unwrap();
					if let Some(_existing_id) = state.local_validator_id {
						Err(jsonrpc_core::Error::invalid_params(
							"Have already self identified",
						))
					} else {
						let validator_id: AccountId = validator_id.into();
						state.local_validator_id = Some(validator_id.clone());
						encode_and_send(
							&self.p2p_network_service,
							P2PMessage::SelfIdentify(validator_id),
							state.peer_to_validator.keys(),
						);
						Ok(200)
					}
				}

				/// Send message to peer
				fn send(&self, validator_id: AccountIdBs58, message: MessageBs58) -> Result<u64> {
					let state = self.state.lock().unwrap();
					check_p2p_message_is_valid(&state, &message)?;
					if let Some(peer_id) = state.validator_to_peer.get(&validator_id.into()) {
						encode_and_send(
							&self.p2p_network_service,
							P2PMessage::Message(message.into()),
							iter::once(peer_id),
						);
						Ok(200)
					} else {
						Err(jsonrpc_core::Error::invalid_params(
							"Cannot send to unidentified account id",
						))
					}
				}

				/// Broadcast message to all known validators on the network
				fn broadcast(&self, message: MessageBs58) -> Result<u64> {
					let state = self.state.lock().unwrap();
					check_p2p_message_is_valid(&state, &message)?;
					encode_and_send(
						&self.p2p_network_service,
						P2PMessage::Message(message.into()),
						state.validator_to_peer.values(),
					);
					Ok(200)
				}

				/// Subscribe to receive P2PEvents
				fn subscribe_notifications(
					&self,
					_metadata: Self::Metadata,
					subscriber: Subscriber<P2PEvent>,
				) {
					let (sender, receiver) = unbounded();
					let subscription_id =
						self.notification_rpc_subscription_manager
							.add(subscriber, |sink| {
								sink.sink_map_err(|e| warn!("Error sending notifications: {:?}", e))
									.send_all(
										receiver.map(|x| Ok::<_, ()>(x)).compat().map(|x| Ok(x)),
									)
									.map(|_| ())
							});
					self.state
						.lock()
						.unwrap()
						.notification_rpc_subscribers
						.insert(subscription_id, sender);
				}

				/// Unsubscribe to stop receiving P2PEvents
				fn unsubscribe_notifications(
					&self,
					_metadata: Option<Self::Metadata>,
					id: SubscriptionId,
				) -> jsonrpc_core::Result<bool> {
					Ok(
						if self
							.notification_rpc_subscription_manager
							.cancel(id.clone())
						{
							self.state
								.lock()
								.unwrap()
								.notification_rpc_subscribers
								.remove(&id)
								.unwrap();
							true
						} else {
							assert!(!self
								.state
								.lock()
								.unwrap()
								.notification_rpc_subscribers
								.contains_key(&id));
							false
						},
					)
				}
			}

			let mut io = jsonrpc_core::MetaIoHandler::default();
			io.extend_with(P2PValidatorNetworkNodeRpcApi::to_delegate(
				RpcRequestHandler {
					state: state.clone(),
					p2p_network_service: p2p_network_service.clone(),
					notification_rpc_subscription_manager: SubscriptionManager::new(Arc::new(
						subscription_task_executor,
					)),
					_phantom: std::marker::PhantomData::<MetaData>::default(),
				},
			));
			io
		},
		// P2P Event Handler
		{
			let mut network_event_stream = p2p_network_service.event_stream();

			fn notify_rpc_subscribers(state: &P2PValidatorNetworkNodeState, event: P2PEvent) {
				for (_subscription_id, sender) in &state.notification_rpc_subscribers {
					if let Err(e) = sender.unbounded_send(event.clone()) {
						debug!("Failed to send message: {:?}", e);
					}
				}
			}

			async move {
				while let Some(event) = network_event_stream.next().await {
					match event {
						Event::SyncConnected { remote } => {
							p2p_network_service.reserve_peer(remote);
						}
						Event::SyncDisconnected { remote } => {
							p2p_network_service.remove_reserved_peer(remote);
						}
						/*A peer has connected to the p2p network*/
						Event::NotificationStreamOpened {
							remote,
							protocol,
							role: _,
						} => {
							if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
								let mut state = state.lock().unwrap();
								state.peer_to_validator.insert(remote, None);
								if let Some(validator_id) = state.local_validator_id {
									encode_and_send(
										&p2p_network_service,
										P2PMessage::SelfIdentify(validator_id),
										iter::once(&remote),
									);
								}
							}
						}
						/*A peer has disconnected from the p2p network*/
						Event::NotificationStreamClosed { remote, protocol } => {
							if protocol == CHAINFLIP_P2P_PROTOCOL_NAME {
								let mut state = state.lock().unwrap();
								if let Some(Some(validator_id)) =
									state.peer_to_validator.remove(&remote)
								{
									state.validator_to_peer.remove(&validator_id).unwrap();
									notify_rpc_subscribers(
										&state,
										P2PEvent::ValidatorDisconnected(validator_id.into()),
									);
								}
							}
						}
						/*Received p2p messages from a peer*/
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
								let mut state = state.lock().unwrap();
								for message in messages {
									match bincode::deserialize(&message) {
										Ok(P2PMessage::SelfIdentify(validator_id)) => {
											match state.peer_to_validator.entry(remote) {
												Entry::Vacant(_entry) => {
													log::warn!(
														"Received an identify before stream opened for peer {:?}",
														remote
													);
												}
												Entry::Occupied(mut entry) => {
													if let Some(_) = entry.get() {
														log::warn!(
															"Received a duplicate identification {:?} for peer {:?}",
															validator_id,
															remote
														);
													} else {
														*entry.get_mut() = Some(validator_id);
														state
															.validator_to_peer
															.insert(validator_id, remote);
														notify_rpc_subscribers(
															&state,
															P2PEvent::ValidatorConnected(
																validator_id.into(),
															),
														);
													}
												}
											}
										}
										Ok(P2PMessage::Message(raw_message)) => {
											match state.peer_to_validator.get(&remote) {
												Some(Some(validator_id)) => {
													notify_rpc_subscribers(
														&state,
														P2PEvent::MessageReceived(
															validator_id.clone().into(),
															raw_message.into(),
														),
													);
												}
												_ => log::error!(
													"Dropping message from unidentified peer {:?}",
													remote
												),
											}
										}
										Err(err) => {
											log::error!("Error deserializing p2p message: {}", err);
										}
									}
								}
							}
						}
						Event::Dht(_) => {}
					}
				}
			}
		},
	)
}

#[cfg(test)]
mod tests {

	use super::*;
	use jsonrpc_core_client::transports::local;
	use tokio;
	use tokio_stream::wrappers::UnboundedReceiverStream;

	struct TestNetwork {
		runtime: tokio::runtime::Runtime,
		validators: Mutex<HashMap<PeerId, tokio::sync::mpsc::UnboundedSender<Event>>>,
	}
	impl TestNetwork {
		fn new() -> Arc<Self> {
			Arc::new(Self {
				runtime: tokio::runtime::Builder::new_multi_thread()
					.enable_all()
					.build()
					.unwrap(),
				validators: Default::default(),
			})
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
			let peer_id = self.peer_id;
			let network = self.network.clone();
			std::thread::spawn(move || {
				let mut validators = network.validators.lock().unwrap();
				network.runtime.block_on(async move {
					validators.remove(&peer_id);
					for remote_sender in validators.values() {
						remote_sender
							.send(Event::NotificationStreamClosed {
								remote: peer_id,
								protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
							})
							.unwrap();
						remote_sender
							.send(Event::SyncDisconnected { remote: peer_id })
							.unwrap();
					}
				});
			})
			.join()
			.unwrap();
		}
	}
	impl PeerNetwork for TestNetworkInterface {
		fn reserve_peer(&self, _who: PeerId) {}

		fn remove_reserved_peer(&self, _who: PeerId) {}

		fn write_notification(&self, who: PeerId, message: Vec<u8>) {
			let peer_id = self.peer_id;
			let network = self.network.clone();
			std::thread::spawn(move || {
				let validators = network.validators.lock().unwrap();
				network.runtime.block_on(async move {
					if let Some(sender) = validators.get(&who) {
						sender
							.send(Event::NotificationsReceived {
								remote: peer_id,
								messages: vec![(CHAINFLIP_P2P_PROTOCOL_NAME, message.into())],
							})
							.unwrap();
					}
				});
			})
			.join()
			.unwrap();
		}

		fn event_stream(&self) -> Pin<Box<dyn futures::Stream<Item = Event> + Send>> {
			let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
			let peer_id = self.peer_id;
			let network = self.network.clone();
			std::thread::spawn(move || {
				let mut validators = network.validators.lock().unwrap();
				network.runtime.block_on(async move {
					for (remote_peer_id, remote_sender) in validators.iter() {
						use sc_network::ObservedRole;

						remote_sender
							.send(Event::SyncConnected { remote: peer_id })
							.unwrap();
						remote_sender
							.send(Event::NotificationStreamOpened {
								remote: peer_id,
								protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
								role: ObservedRole::Full,
							})
							.unwrap();
						sender
							.send(Event::SyncConnected {
								remote: *remote_peer_id,
							})
							.unwrap();
						sender
							.send(Event::NotificationStreamOpened {
								remote: *remote_peer_id,
								protocol: CHAINFLIP_P2P_PROTOCOL_NAME,
								role: ObservedRole::Full,
							})
							.unwrap();
					}

					use std::collections::hash_map;
					match validators.entry(peer_id) {
						hash_map::Entry::Occupied(_entry) => Err(()),
						hash_map::Entry::Vacant(entry) => Ok(entry.insert(sender)),
					}
					.unwrap(); // Assumed to be called once
				});
			})
			.join()
			.unwrap();
			Box::pin(UnboundedReceiverStream::new(receiver))
		}
	}

	fn setup_test(peer_id: PeerId, network: Arc<TestNetwork>) -> P2PRpcClient {
		let (rpc_request_handler, p2p_event_handler_fut) = std::thread::spawn(move || {
			new_p2p_validator_network_node(
				Arc::new(TestNetworkInterface::new(peer_id, network)),
				sc_rpc::testing::TaskExecutor,
			)
		})
		.join()
		.unwrap();
		let rpc_request_handler = Arc::new(rpc_request_handler);
		let (client, server) = local::connect_with_pubsub::<P2PRpcClient, _>(rpc_request_handler);

		use futures::compat::Future01CompatExt;

		tokio::runtime::Handle::current().spawn(server.compat());
		tokio::runtime::Handle::current().spawn(p2p_event_handler_fut);

		client
	}

	#[tokio::test]
	async fn repeat_self_identify_fails() {
		use futures::compat::Future01CompatExt;

		let network = TestNetwork::new();
		let node_0 = setup_test(PeerId::random(), network.clone());

		node_0
			.self_identify(AccountIdBs58([0; 32]))
			.compat()
			.await
			.unwrap();
		node_0
			.self_identify(AccountIdBs58([0; 32]))
			.compat()
			.await
			.unwrap_err();
	}
}
