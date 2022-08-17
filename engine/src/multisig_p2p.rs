use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet},
    net::Ipv6Addr,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use futures::stream::StreamExt;
use itertools::Itertools;
use jsonrpsee::ws_client::WsClientBuilder;
use lazy_format::lazy_format;
use multisig_p2p_transport::{PeerId, PeerIdTransferable};
use slog::o;
use sp_core::{storage::StorageKey, H256};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub use multisig_p2p_transport::P2PValidatorNetworkNodeRpcApiClient;
use state_chain_runtime::AccountId;

use codec::Encode;

use zeroize::Zeroizing;

use frame_support::StoragePrefixedMap;

use crate::{
    common::{self, format_iterator, read_clean_and_decode_hex_str_file},
    logging::COMPONENT_KEY,
    settings,
    state_chain_observer::client::{StateChainClient, StateChainRpcApi},
};

use utilities::{make_periodic_tick, Port};

#[derive(Debug)]
pub enum AccountPeerMappingChange {
    Registered(Port, Ipv6Addr),
    Unregistered,
}

// TODO: Consider if this should be removed, particularly once we no longer use Substrate for peering
#[derive(Debug, PartialEq)]
pub enum OutgoingMultisigStageMessages {
    Broadcast(Vec<AccountId>, Vec<u8>),
    Private(Vec<(AccountId, Vec<u8>)>),
}

/*
TODO: This code should be merged into the multisig top-level function (start_client),
primarily to avoid the problem where multisig sends messages before the mapping
has been updated, which is possible at the moment.
TODO: Flip port and ip_address ordering in parameters (Everywhere)
*/

async fn update_registered_peer_id<RpcClient: 'static + StateChainRpcApi + Sync + Send>(
    peer_id_from_cfe_config: &PeerId,
    peer_keypair_from_cfe_config: &libp2p::identity::ed25519::Keypair,
    state_chain_client: &Arc<StateChainClient<RpcClient>>,
    account_to_peer_mapping_on_chain: &BTreeMap<AccountId, (PeerId, Port, Ipv6Addr)>,
    logger: &slog::Logger,
) -> Result<()> {
    // TODO Don't Register Private Ips on Live chains

    let listening_addresses = state_chain_client
        .get_local_listen_addresses()
        .await?
        .into_iter()
        .filter(|(_, _, ip_address)| !ip_address.is_loopback())
        .sorted()
        .dedup()
        .collect::<Vec<_>>();

    if listening_addresses.is_empty() {
        Err(anyhow!("No non-loopback listening addresses reported",))
    } else if let Some(&peer_id_from_node) =
        common::all_same(listening_addresses.iter().map(|(peer_id, _, _)| peer_id))
    {
        if *peer_id_from_cfe_config == peer_id_from_node {
            let (port, ip_address, source) = if let Some((port, ip_address)) = listening_addresses
                .iter()
                .find(|(_, _, ipv6_address)|
                    // Ipv6Addr::is_global doesn't handle Ipv4 mapped addresses
                    match ipv6_address.to_ipv4_mapped() {
                        Some(ipv4_address) => ipv4_address.is_global(),
                        None => ipv6_address.is_global(),
                    })
                .map(|(_, port, ip_address)| (*port, *ip_address))
            {
                (
                    port,
                    ip_address,
                    "a public ip selected from the node's reported listening addresses",
                )
            } else if let Some((port, ip_address)) = {
                slog::warn!(logger, "The node is not reporting a public ip address");

                if let Some(public_ip) = public_ip::addr().await {
                    // We don't know which private ip address the public ip address maps to,
                    // so we must pick a port number that is listened to on all the private ip's to ensure it is correct
                    if let Some(port) = listening_addresses
                        .iter()
                        .map(|(_, port, ip_address)| (ip_address, port))
                        .sorted()
                        .group_by(|(ip_address, _)| *ip_address)
                        .into_iter()
                        .map(|(_, group)| group.map(|(_, port)| *port).collect::<BTreeSet<_>>())
                        .reduce(|ports_a, ports_b| {
                            ports_a.intersection(&ports_b).cloned().collect()
                        })
                        .unwrap()
                        .into_iter()
                        .next()
                    {
                        Some((
                            port,
                            match public_ip {
                                std::net::IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                                std::net::IpAddr::V6(ip) => ip,
                            },
                        ))
                    } else {
                        slog::warn!(logger, "We could not determine the correct port number for the resolved public ip address {}", public_ip);
                        None
                    }
                } else {
                    slog::warn!(logger, "We could not resolve the node's public ip address");
                    None
                }
            } {
                (port, ip_address, "the node's resolved public address")
            } else {
                let (_, port, ip_address) = listening_addresses.first().unwrap();
                (
                    *port,
                    *ip_address,
                    "a private address selected from the node's listening addresses",
                )
            };

            let option_previous_address_on_chain =
                account_to_peer_mapping_on_chain.get(&state_chain_client.our_account_id);

            if Some(&(peer_id_from_node, port, ip_address)) != option_previous_address_on_chain {
                slog::info!(
                    logger,
                    "Node's reported listening addresses: {}",
                    common::format_iterator(
                        listening_addresses
                            .iter()
                            .map(|(_, port, ip_address)| format!("[{}]:{}", ip_address, port))
                    )
                );
                slog::info!(
                    logger,
                    "Registering node's peer_id {}, ip address, and port number [{}]:{}. This ip address is {}. {}.",
                    peer_id_from_node,
                    ip_address,
                    port,
                    source,
                    lazy_format!(match (option_previous_address_on_chain) {
                        Some(&(previous_peer_id, previous_port, previous_ip_address)) => (
                            "Node was previously registered with peer_id {}, ip address, and port number [{}]:{}",
                            previous_peer_id,
                            previous_ip_address,
                            previous_port
                        ),
                        None => ("Node previously did not have a registered address")
                    })
                );
                state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_validator::Call::register_peer_id {
                            peer_id: sp_core::ed25519::Public(
                                peer_keypair_from_cfe_config.public().encode(),
                            ),
                            port,
                            ip_address: ip_address.into(),
                            signature: sp_core::ed25519::Signature::try_from(
                                &peer_keypair_from_cfe_config
                                    .sign(&state_chain_client.our_account_id.encode()[..])[..],
                            )
                            .unwrap(),
                        },
                        logger,
                    )
                    .await?;
            }

            Ok(())
        } else {
            Err(anyhow!("Your Chainflip Node is using a different peer id ({}) than you provided to your Chainflip Engine ({}). Check the p2p.node_key_file configuration option.", peer_id_from_node, peer_id_from_cfe_config))
        }
    } else {
        Err(anyhow!("Cannot select which peer_id to register as the Chainflip Node is reporting multiple different peer_ids"))
    }
}

fn public_key_to_peer_id(peer_public_key: &sp_core::ed25519::Public) -> PeerId {
    PeerId::from_public_key(
        &libp2p::identity::PublicKey::Ed25519(libp2p::identity::ed25519::PublicKey::decode(
            &peer_public_key.0[..],
        ).expect("peer_id's are checked before being registered using ed25519_dalek::PublicKey::from_bytes, the same function used to decode and check peer_id's inside libp2p::identity::ed25519::PublicKey::decode")),
    )
}

pub async fn start<RpcClient: 'static + StateChainRpcApi + Sync + Send>(
    settings: &settings::Settings,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    latest_block_hash: H256,
    incoming_p2p_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
    mut outgoing_p2p_message_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    mut account_mapping_change_receiver: UnboundedReceiver<(
        AccountId,
        sp_core::ed25519::Public,
        AccountPeerMappingChange,
    )>,
    logger: &slog::Logger,
) -> Result<()> {
    let logger = logger.new(o!(COMPONENT_KEY => "P2PClient"));

    let client = WsClientBuilder::default()
        // TODO: consider adding something like this
        // .max_request_body_size(TEN_MB_SIZE_BYTES * X)
        .build(settings.state_chain.ws_endpoint.as_str())
        .await?;

    let mut account_to_peer_mapping_on_chain = state_chain_client
        .get_storage_pairs::<(AccountId, sp_core::ed25519::Public, Port, pallet_cf_validator::Ipv6Addr)>(
            latest_block_hash,
            StorageKey(
                pallet_cf_validator::AccountPeerMapping::<state_chain_runtime::Runtime>::final_prefix()
                    .into(),
            ),
        )
        .await?
        .into_iter()
        .map(|(account_id, public_key, port, ip_address)| {
            (
                account_id,
                (
                    public_key_to_peer_id(&public_key),
                    port,
                    ip_address.into(),
                )
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut peer_to_account_mapping_on_chain = account_to_peer_mapping_on_chain
        .iter()
        .map(|(account_id, (peer_id, _, _))| (*peer_id, account_id.clone()))
        .collect::<BTreeMap<_, _>>();

    slog::info!(
        logger,
        "Loaded account peer mapping from chain: {:?}",
        account_to_peer_mapping_on_chain
    );

    let peer_keypair_from_cfe_config: libp2p::identity::ed25519::Keypair =
        read_clean_and_decode_hex_str_file(&settings.node_p2p.node_key_file, "Node Key", |str| {
            libp2p::identity::ed25519::SecretKey::from_bytes(
                &mut Zeroizing::new(hex::decode(str).map_err(anyhow::Error::new)?)[..],
            )
            .map_err(anyhow::Error::new)
        })?
        .into();
    let peer_id_from_cfe_config =
        libp2p::identity::PublicKey::Ed25519(peer_keypair_from_cfe_config.public()).to_peer_id();
    update_registered_peer_id(
        &peer_id_from_cfe_config,
        &peer_keypair_from_cfe_config,
        &state_chain_client,
        &account_to_peer_mapping_on_chain,
        &logger,
    )
    .await?;

    client
        .set_peers(
            account_to_peer_mapping_on_chain
                .values()
                .filter_map(|(peer_id, port, ip_address)| {
                    if peer_id_from_cfe_config != *peer_id {
                        Some((PeerIdTransferable::from(peer_id), *port, *ip_address))
                    } else {
                        None
                    }
                })
                .collect(),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to add peers to reserved set: {:?}",
                account_to_peer_mapping_on_chain
            )
        })?;
    slog::info!(
        logger,
        "Added peers to reserved set: {:?}",
        account_to_peer_mapping_on_chain
    );

    let (ipc_outgoing_sender, mut ipc_incoming_stream) =
        multisig_p2p_transport::setup_ipc_connections(&client).await?;

    let mut check_listener_address_tick = make_periodic_tick(Duration::from_secs(60), false);

    let (internal_incoming_sender, mut internal_incoming_receiver) =
        tokio::sync::mpsc::unbounded_channel::<(PeerId, Vec<u8>)>();
    tokio::task::spawn(async move {
        while let Some((peer_id, serialised_message)) = ipc_incoming_stream.next().await {
            let peer_id: PeerId = peer_id.try_into()?;
            let _result = internal_incoming_sender.send((peer_id, serialised_message));
        }
        anyhow::Result::<()>::Ok(())
    });

    let (internal_outgoing_sender, internal_outgoing_receiver) =
        std::sync::mpsc::channel::<(Vec<PeerIdTransferable>, Vec<u8>)>();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        while let Ok((peer_ids, message)) = internal_outgoing_receiver.recv() {
            ipc_outgoing_sender.send((peer_ids, message)).unwrap();
        }
        Ok(())
    });

    loop {
        tokio::select! {
            Some((peer_id, serialised_message)) = internal_incoming_receiver.recv() => {
                match peer_to_account_mapping_on_chain.get(&peer_id).ok_or_else(|| anyhow!("Missing Account Id mapping for Peer Id: {peer_id}")).and_then(|account_id| {
                    incoming_p2p_message_sender.send((
                        account_id.clone(),
                        serialised_message
                    )).map_err(|error| anyhow!("Failed to send message via channel: {error}"))?;
                    Ok(account_id)
                }) {
                    Ok(account_id) => slog::info!(logger, "Received P2P message from: {}", account_id),
                    Err(error) => slog::error!(logger, "Failed to receive P2P message: {}", error)
                }
            }
            Some(messages) = outgoing_p2p_message_receiver.recv() => {
                let send_messages = |
                    account_ids: &[AccountId],
                    message: Vec<u8>,
                    logger: &slog::Logger
                | {
                    // TODO: If we can't get peer id for some accounts, should we still try to send messages to the rest?
                    match
                        account_ids
                            .iter()
                            .map(|account_id| match account_to_peer_mapping_on_chain.get(account_id) {
                                Some((peer_id, _, _)) => Ok(peer_id.into()),
                                None => Err(anyhow!("Missing Peer Id mapping for Account Id: {}", account_id)),
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .and_then(|peer_ids| {
                                internal_outgoing_sender.send((peer_ids, message)).map_err(anyhow::Error::new)
                            })
                    {
                        Ok(_) => slog::info!(logger, "Sent P2P message to: {}", format_iterator(account_ids)),
                        Err(error) => slog::error!(logger, "Failed to send P2P message to: {}. {}", format_iterator(account_ids), error)
                    }
                };

                match messages {
                    OutgoingMultisigStageMessages::Broadcast(account_ids, message) => {
                        send_messages(&account_ids, message, &logger);
                    },
                    OutgoingMultisigStageMessages::Private(messages) => {
                        for (account_id, message) in messages {
                            send_messages(&[account_id], message, &logger);
                        }
                    }
                }
            }
            Some((account_id, peer_public_key, account_peer_mapping_change)) = account_mapping_change_receiver.recv() => {
                let peer_id = public_key_to_peer_id(&peer_public_key);
                match account_peer_mapping_change {
                    AccountPeerMappingChange::Registered(port, ip_address) => {
                        if let Some((existing_peer_id, _, _)) = account_to_peer_mapping_on_chain.get(&account_id) {
                            peer_to_account_mapping_on_chain.remove(existing_peer_id);
                        }
                        if let Entry::Vacant(entry) = peer_to_account_mapping_on_chain.entry(peer_id) {
                            entry.insert(account_id.clone());
                            account_to_peer_mapping_on_chain.insert(account_id, (peer_id, port, ip_address));
                            if peer_id_from_cfe_config != peer_id {
                                if let Err(error) = client.add_peer(PeerIdTransferable::from(&peer_id), port, ip_address).await {
                                    slog::error!(logger, "Couldn't add peer {} to reserved set: {}", peer_id, error);
                                } else {
                                    slog::info!(logger, "Added peer {} to reserved set", peer_id);
                                }
                            }
                        } else {
                            slog::error!(logger, "Unexpected Peer Registered event received for {} (Peer id: {}).", account_id, peer_id);
                        }
                    }
                    AccountPeerMappingChange::Unregistered => {
                        if Some(&account_id) == peer_to_account_mapping_on_chain.get(&peer_id) {
                            account_to_peer_mapping_on_chain.remove(&account_id);
                            peer_to_account_mapping_on_chain.remove(&peer_id);
                            if peer_id_from_cfe_config != peer_id {
                                if let Err(error) = client.remove_peer(PeerIdTransferable::from(&peer_id)).await {
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
            _ = check_listener_address_tick.tick() => {
                update_registered_peer_id(&peer_id_from_cfe_config, &peer_keypair_from_cfe_config, &state_chain_client, &account_to_peer_mapping_on_chain, &logger).await?;
            }
        }
    }
}
