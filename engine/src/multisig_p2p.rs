use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    net::Ipv6Addr,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use futures::TryStreamExt;
use itertools::Itertools;
use multisig_p2p_transport::{PeerId, PeerIdTransferable};
use slog::o;
use sp_core::{storage::StorageKey, H256};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub use multisig_p2p_transport::P2PRpcClient;
use state_chain_runtime::AccountId;

use codec::Encode;

use futures::{StreamExt, TryFutureExt};
use zeroize::Zeroizing;

use frame_support::StoragePrefixedMap;

use crate::{
    common::{self, read_clean_and_decode_hex_str_file, rpc_error_into_anyhow_error},
    logging::COMPONENT_KEY,
    multisig::MultisigMessage,
    settings,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};

#[derive(Debug)]
pub enum AccountPeerMappingChange {
    Registered(u16, Ipv6Addr),
    Unregistered,
}

/*
TODO: This code should be merged into the multisig top-level function (start_client),
primarily to avoid the problem where multisig sends messages before the mapping
has been updated, which is possible at the moment.
TODO: Batch outgoing messages
*/

async fn update_registered_peer_id<RPCClient: 'static + StateChainRpcApi + Sync + Send>(
    cfe_peer_id: &PeerId,
    cfe_peer_keypair: &libp2p::identity::ed25519::Keypair,
    state_chain_client: &Arc<StateChainClient<RPCClient>>,
    account_to_peer: &BTreeMap<AccountId, (PeerId, u16, Ipv6Addr)>,
    logger: &slog::Logger,
) -> Result<()> {
    let (peer_id, port, ip_address) = state_chain_client
        .get_local_listen_addresses()
        .await?
        .into_iter()
        .sorted()
        .dedup()
        .filter(|(_, _, ip_address)| !ip_address.is_loopback())
        .next()
        .ok_or_else(|| anyhow::Error::msg("Couldn't find the node's listening address"))?;

    if *cfe_peer_id == peer_id {
        if Some(&(peer_id, port, ip_address))
            != account_to_peer.get(&state_chain_client.our_account_id)
        {
            state_chain_client
                .submit_signed_extrinsic(
                    logger,
                    pallet_cf_validator::Call::register_peer_id(
                        sp_core::ed25519::Public(cfe_peer_keypair.public().encode()),
                        port,
                        ip_address.into(),
                        sp_core::ed25519::Signature::try_from(
                            &cfe_peer_keypair.sign(&state_chain_client.our_account_id.encode()[..])
                                [..],
                        )
                        .unwrap(),
                    ),
                )
                .await?;
        }

        Ok(())
    } else {
        Err(anyhow::Error::msg(format!("Your Chainflip Node is using a different peer id ({}) than you provided to your Chainflip Engine ({}). Check the p2p.node_key_file confugration option.", peer_id, cfe_peer_id)))
    }
}

fn public_key_to_peer_id(peer_public_key: &sp_core::ed25519::Public) -> Result<PeerId> {
    Ok(PeerId::from_public_key(
        libp2p::identity::PublicKey::Ed25519(libp2p::identity::ed25519::PublicKey::decode(
            &peer_public_key.0[..],
        )?),
    ))
}

pub async fn start<RPCClient: 'static + StateChainRpcApi + Sync + Send>(
    settings: &settings::Settings,
    state_chain_client: Arc<StateChainClient<RPCClient>>,
    latest_block_hash: H256,
    incoming_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
    mut outgoing_p2p_message_receiver: UnboundedReceiver<(AccountId, MultisigMessage)>,
    mut account_mapping_change_receiver: UnboundedReceiver<(
        AccountId,
        sp_core::ed25519::Public,
        AccountPeerMappingChange,
    )>,
    logger: &slog::Logger,
) -> Result<()> {
    let logger = logger.new(o!(COMPONENT_KEY => "P2PClient"));

    // Use StateChainClient's RpcChannel
    let client = jsonrpc_core_client::transports::ws::connect::<P2PRpcClient>(
        &url::Url::parse(settings.state_chain.ws_endpoint.as_str()).with_context(|| {
            format!(
                "Should be valid ws endpoint: {}",
                settings.state_chain.ws_endpoint
            )
        })?,
    )
    .await
    .map_err(rpc_error_into_anyhow_error)?;

    let mut account_to_peer = state_chain_client
        .get_storage_pairs::<(AccountId, sp_core::ed25519::Public, u16, pallet_cf_validator::Ipv6Addr)>(
            latest_block_hash,
            StorageKey(
                pallet_cf_validator::AccountPeerMapping::<state_chain_runtime::Runtime>::final_prefix()
                    .into(),
            ),
        )
        .await?
        .into_iter()
        .map(|(account_id, public_key, port, ip_address)| {
            Ok((
                account_id,
                (
                    public_key_to_peer_id(&public_key)?,
                    port,
                    ip_address.into(),
                )
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;

    let mut peer_to_account = account_to_peer
        .iter()
        .map(|(account_id, (peer_id, _, _))| (peer_id.clone(), account_id.clone()))
        .collect::<BTreeMap<_, _>>();

    slog::info!(
        logger,
        "Loaded account peer mapping from chain: {:#?}",
        account_to_peer
    );

    let cfe_peer_keypair: libp2p::identity::ed25519::Keypair =
        read_clean_and_decode_hex_str_file(&settings.node_p2p.node_key_file, "Node Key", |str| {
            libp2p::identity::ed25519::SecretKey::from_bytes(
                &mut Zeroizing::new(hex::decode(str).map_err(anyhow::Error::new)?)[..],
            )
            .map_err(anyhow::Error::new)
        })?
        .into();
    let cfe_peer_id =
        libp2p::identity::PublicKey::Ed25519(cfe_peer_keypair.public()).into_peer_id();
    update_registered_peer_id(
        &cfe_peer_id,
        &cfe_peer_keypair,
        &state_chain_client,
        &account_to_peer,
        &logger,
    )
    .await?;

    client
        .set_peers(
            account_to_peer
                .values()
                .filter_map(|(peer_id, port, ip_address)| {
                    if cfe_peer_id != *peer_id {
                        Some((PeerIdTransferable::from(peer_id), *port, *ip_address))
                    } else {
                        None
                    }
                })
                .collect(),
        )
        .await
        .map_err(rpc_error_into_anyhow_error)
        .with_context(|| {
            format!(
                "Failed to add peers to reserved set: {:#?}",
                account_to_peer
            )
        })?;
    slog::info!(
        logger,
        "Added peers to reserved set: {:#?}",
        account_to_peer
    );

    let mut incoming_p2p_message_stream = client
        .subscribe_messages()
        .map_err(rpc_error_into_anyhow_error)?
        .map_err(rpc_error_into_anyhow_error);

    let mut check_listener_address_stream = common::make_periodic_stream(Duration::from_secs(60));

    loop {
        tokio::select! {
            Some(result_p2p_message) = incoming_p2p_message_stream.next() => {
                match result_p2p_message.and_then(|(peer_id, serialised_message)| {
                    let peer_id: PeerId = peer_id.try_into()?;
                    if let Some(account_id) = peer_to_account.get(&peer_id) {
                        incoming_p2p_message_sender.send((
                            account_id.clone(),
                            bincode::deserialize::<MultisigMessage>(&serialised_message[..]).with_context(|| format!("Failed to deserialise message from Validator {}.", account_id))?
                        )).map_err(anyhow::Error::new).with_context(|| format!("Failed to send message via channel"))?;
                        Ok(account_id)
                    } else {
                        Err(anyhow::Error::msg(format!("Missing Account Id mapping for Peer Id: {}", peer_id)))
                    }
                }) {
                    Ok(account_id) => slog::info!(logger, "Received P2P message from: {}", account_id),
                    Err(error) => slog::error!(logger, "Failed to receive P2P message: {}", error)
                }
            }
            Some((account_id, message)) = outgoing_p2p_message_receiver.recv() => {
                match async {
                    account_to_peer.get(&account_id).ok_or_else(|| anyhow::Error::msg(format!("Missing Peer Id mapping for Account Id: {}", account_id)))
                }.and_then(|(peer_id, _, _)| {
                    client.send_message(
                        vec![peer_id.into()],
                        bincode::serialize(&message).unwrap()
                    ).map_err(rpc_error_into_anyhow_error)
                }).await {
                    Ok(_) => slog::info!(logger, "Sent P2P message to: {}", account_id),
                    Err(error) => slog::error!(logger, "Failed to send P2P message to: {}. {}", account_id, error)
                }
            }
            Some((account_id, peer_public_key, account_peer_mapping_change)) = account_mapping_change_receiver.recv() => {
                match public_key_to_peer_id(&peer_public_key) {
                    Ok(peer_id) => {
                        match account_peer_mapping_change {
                            AccountPeerMappingChange::Registered(port, ip_address) => {
                                if let Some((existing_peer_id, _, _)) = account_to_peer.get(&account_id) {
                                    peer_to_account.remove(&existing_peer_id);
                                }
                                if peer_to_account.contains_key(&peer_id) {
                                    slog::error!(logger, "Unexpected Peer Registered event received for {} (Peer id: {}).", account_id, peer_id);
                                } else {
                                    peer_to_account.insert(peer_id.clone(), account_id.clone());
                                    account_to_peer.insert(account_id, (peer_id.clone(), port, ip_address));
                                    if cfe_peer_id != peer_id {
                                        if let Err(error) = client.add_peer(PeerIdTransferable::from(&peer_id), port, ip_address).await.map_err(rpc_error_into_anyhow_error) {
                                            slog::error!(logger, "Couldn't add peer {} to reserved set: {}", peer_id, error);
                                        } else {
                                            slog::info!(logger, "Added peer {} to reserved set", peer_id);
                                        }
                                    }
                                }
                            }
                            AccountPeerMappingChange::Unregistered => {
                                if Some(&account_id) == peer_to_account.get(&peer_id) {
                                    account_to_peer.remove(&account_id);
                                    peer_to_account.remove(&peer_id);
                                    if cfe_peer_id != peer_id {
                                        if let Err(error) = client.remove_peer(PeerIdTransferable::from(&peer_id)).await.map_err(rpc_error_into_anyhow_error) {
                                            slog::error!(logger, "Couldn't remove peer {} to reserved set: {}", peer_id, error);
                                        } else {
                                            slog::info!(logger, "Removed peer {} to reserved set", peer_id);
                                        }
                                    }
                                } else {
                                    slog::error!(logger, "Unexpected Peer Unregistered event received for {} (Peer id: {}).", account_id, peer_id);
                                }
                            }
                        }
                    },
                    Err(error) => slog::error!(logger, "Unable to convert public key {} to peer id. {}", peer_public_key, error)
                }
            },
            Some(()) = check_listener_address_stream.next() => {
                update_registered_peer_id(&cfe_peer_id, &cfe_peer_keypair, &state_chain_client, &account_to_peer, &logger).await?;
            }
        }
    }
}

/*
OLD TESTS TO CHANGE

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use itertools::Itertools;

    use super::mock::*;
    use super::*;

    async fn receive_with_timeout<T>(mut stream: BoxStream<'_, T>) -> Option<T> {
        let fut = stream.next();
        tokio::time::timeout(std::time::Duration::from_millis(5), fut)
            .await
            .unwrap_or(None)
    }

    #[tokio::test]
    async fn test_p2p_mock_send() {
        let network = NetworkMock::new();

        let data = vec![1, 2, 3];
        let validator_ids = (0..3).map(|i| AccountId([i; 32])).collect_vec();

        let clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (0) sends to (1); (1) should receive one, (2) receives none
        clients[0].send(&validator_ids[1], &data).await.unwrap();

        drop(network);

        let stream_1 = clients[1].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_1).await,
            Some(P2PMessage {
                account_id: validator_ids[0].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(receive_with_timeout(stream_2).await, None);
    }

    #[tokio::test]
    async fn test_p2p_mock_broadcast() {
        let network = NetworkMock::new();

        let data = vec![3, 2, 1];
        let validator_ids = (0..3).map(|i| AccountId([i; 32])).collect_vec();
        let clients = validator_ids
            .iter()
            .map(|id| network.new_client(id.clone()))
            .collect_vec();

        // (1) broadcasts; (0) and (2) should receive one message
        clients[1].broadcast(&data).await.unwrap();

        let stream_0 = clients[0].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_0).await,
            Some(P2PMessage {
                account_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );

        let stream_2 = clients[2].take_stream().await.unwrap();

        assert_eq!(
            receive_with_timeout(stream_2).await,
            Some(P2PMessage {
                account_id: validator_ids[1].clone(),
                data: data.clone()
            })
        );
    }
}

pub struct P2PClientMock {
    id: AccountId,
    pub receiver: Arc<Mutex<Option<UnboundedReceiverStream<P2PMessage>>>>,
    network_inner: Arc<Mutex<NetworkMockInner>>,
}

impl P2PClientMock {
    pub fn new(id: AccountId, network_inner: Arc<Mutex<NetworkMockInner>>) -> Self {
        let (sender, receiver) = unbounded_channel();

        network_inner.lock().unwrap().register(&id, sender);

        P2PClientMock {
            id,
            receiver: Arc::new(Mutex::new(Some(UnboundedReceiverStream::new(receiver)))),
            network_inner,
        }
    }
}

#[async_trait]
impl P2PNetworkClient for P2PClientMock {
    type NetworkEvent = P2PMessage;

    async fn broadcast(&self, data: &[u8]) -> Result<StatusCode> {
        self.network_inner.lock().unwrap().broadcast(&self.id, data);
        Ok(200)
    }

    async fn send(&self, to: &AccountId, data: &[u8]) -> Result<StatusCode> {
        self.network_inner.lock().unwrap().send(&self.id, to, data);
        Ok(200)
    }

    async fn take_stream(&self) -> Result<BoxStream<Self::NetworkEvent>> {
        let stream = self
            .receiver
            .lock()
            .unwrap()
            .take()
            .ok_or(anyhow!("Subscription Error"))?;

        Ok(Box::pin(stream))
    }
}

pub struct MockChannelEventHandler(UnboundedSender<P2PMessage>);

impl MockChannelEventHandler {
    pub fn new() -> (Self, UnboundedReceiver<P2PMessage>) {
        let (s, r) = unbounded_channel();
        (Self(s), r)
    }
}

#[async_trait]
impl NetworkEventHandler<P2PClientMock> for MockChannelEventHandler {
    async fn handle_event(&self, event: P2PMessage) {
        self.0.send(event).unwrap()
    }
}

pub struct NetworkMock(Arc<Mutex<NetworkMockInner>>);

impl NetworkMock {
    pub fn new() -> Self {
        let inner = NetworkMockInner::new();
        let inner = Arc::new(Mutex::new(inner));

        NetworkMock(inner)
    }

    pub fn new_client(&self, id: AccountId) -> P2PClientMock {
        P2PClientMock::new(id, Arc::clone(&self.0))
    }
}

pub struct NetworkMockInner {
    clients: HashMap<AccountId, UnboundedSender<P2PMessage>>,
}

impl NetworkMockInner {
    fn new() -> Self {
        NetworkMockInner {
            clients: HashMap::new(),
        }
    }

    /// Register validator, so we know how to contact them
    fn register(&mut self, id: &AccountId, sender: UnboundedSender<P2PMessage>) {
        let added = self.clients.insert(id.to_owned(), sender).is_none();
        assert!(added, "Cannot insert the same validator more than once");
    }

    fn broadcast(&self, from: &AccountId, data: &[u8]) {
        let m = P2PMessage {
            account_id: from.to_owned(),
            data: data.to_owned(),
        };

        for (id, sender) in &self.clients {
            // Do not send to ourselves
            if id != from {
                match sender.send(m.clone()) {
                    Ok(()) => (),
                    Err(_) => {
                        panic!("channel is disconnected");
                    }
                }
            }
        }
    }

    /// Send to a specific `validator` only
    fn send(&self, from: &AccountId, to: &AccountId, data: &[u8]) {
        let m = P2PMessage {
            account_id: from.to_owned(),
            data: data.to_owned(),
        };

        match self.clients.get(to) {
            Some(client) => match client.send(m) {
                Ok(()) => {}
                Err(_) => {
                    panic!("channel is disconnected");
                }
            },
            None => {
                eprintln!("Client not connected: {}", to);
            }
        }
    }
}


*/
