use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use slog::o;
use sp_core::H256;

use state_chain_runtime::AccountId;

use codec::Encode;
use utilities::make_periodic_tick;

use crate::{
    logging::COMPONENT_KEY,
    p2p::PeerInfo,
    state_chain_observer::client::{
        ChainflipClient, StateChainClient, StateChainRpcApi, StateChainRpcClient,
    },
};

// TODO: Consider if this should be removed, particularly once we no longer use Substrate for peering
#[derive(Debug, PartialEq, Eq)]
pub enum OutgoingMultisigStageMessages {
    Broadcast(Vec<AccountId>, Vec<u8>),
    Private(Vec<(AccountId, Vec<u8>)>),
}

/*
TODO: This code should be merged into the multisig top-level function (start_client),
primarily to avoid the problem where multisig sends messages before the mapping
has been updated, which is possible at the moment.
*/

async fn update_registered_peer_id<RpcClient: 'static + StateChainRpcApi + Sync + Send>(
    node_key: &ed25519_dalek::Keypair,
    state_chain_client: &Arc<StateChainClient<RpcClient>>,
    previous_registered_peer_info: &mut Option<PeerInfo>,
    cfe_port: u16,
    logger: &slog::Logger,
) -> Result<()> {
    // TODO Don't Register Private Ips on Live chains

    // TODO: stop relying on SC for obtaining our IP address and
    // instead have the operator provided it in the config
    let listening_addresses = state_chain_client
        .get_local_listen_addresses()
        .await?
        .into_iter()
        .filter(|ip_address| !ip_address.is_loopback())
        .sorted()
        .dedup()
        .collect::<Vec<_>>();

    if listening_addresses.is_empty() {
        return Err(anyhow!("No non-loopback listening addresses reported",));
    }

    // TODO: check that pubkey hasn't changed?
    let (resolved_ip_address, source) = if let Some(ip_address) =
        listening_addresses.iter().find(|ipv6_address| {
            // Ipv6Addr::is_global doesn't handle Ipv4 mapped addresses
            match ipv6_address.to_ipv4_mapped() {
                Some(ipv4_address) => ipv4_address.is_global(),
                None => ipv6_address.is_global(),
            }
        }) {
        (
            *ip_address,
            "a public ip selected from the node's reported listening addresses",
        )
    } else if let Some(ip_address) = {
        slog::warn!(logger, "The node is not reporting a public ip address");

        if let Some(public_ip) = public_ip::addr().await {
            Some(match public_ip {
                std::net::IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                std::net::IpAddr::V6(ip) => ip,
            })
        } else {
            slog::warn!(logger, "We could not resolve the node's public ip address");
            None
        }
    } {
        (ip_address, "the node's resolved public address")
    } else {
        let ip_address = listening_addresses.first().unwrap();
        (
            *ip_address,
            "a private address selected from the node's listening addresses",
        )
    };

    if Some((resolved_ip_address, cfe_port))
        != previous_registered_peer_info
            .as_ref()
            .map(|pi| (pi.ip, pi.port))
    {
        let extra_info = match previous_registered_peer_info.as_ref() {
            Some(peer_info) => {
                format!(
                    "Node was previously registered with address [{}]:{}",
                    peer_info.ip, peer_info.port
                )
            }
            None => String::from("Node previously did not have a registered address"),
        };

        slog::info!(
            logger,
            "Registering node's ip address, and port number [{}]:{}. This ip address is {}. {}.",
            resolved_ip_address,
            cfe_port,
            source,
            extra_info,
        );

        let peer_id = sp_core::ed25519::Public(node_key.public.to_bytes());

        let signature = {
            use ed25519_dalek::Signer;
            let payload = &state_chain_client.our_account_id.encode();
            node_key.sign(payload)
        };

        state_chain_client
            .submit_signed_extrinsic(
                pallet_cf_validator::Call::register_peer_id {
                    peer_id,
                    port: cfe_port,
                    ip_address: resolved_ip_address.into(),
                    // We sign over our account id
                    signature: sp_core::ed25519::Signature::try_from(signature.as_ref()).unwrap(),
                },
                logger,
            )
            .await?;

        *previous_registered_peer_info = Some(PeerInfo::new(
            state_chain_client.our_account_id.clone(),
            peer_id,
            resolved_ip_address,
            cfe_port,
        ));
    }

    Ok(())
}

pub async fn start<RpcClient: 'static + StateChainRpcApi + Sync + Send>(
    node_key: ed25519_dalek::Keypair,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    cfe_port: u16,
    mut previous_registered_peer_info: Option<PeerInfo>,
    logger: &slog::Logger,
) -> Result<()> {
    let logger = logger.new(o!(COMPONENT_KEY => "P2PClient"));

    let mut update_interval = make_periodic_tick(Duration::from_secs(60), false);

    loop {
        update_registered_peer_id(
            &node_key,
            &state_chain_client,
            &mut previous_registered_peer_info,
            cfe_port,
            &logger,
        )
        .await?;

        update_interval.tick().await;
    }
}

pub async fn get_current_peer_infos(
    state_chain_client: &Arc<
        StateChainClient<StateChainRpcClient<impl ChainflipClient + Send + Sync>>,
    >,
    block_hash: H256,
) -> anyhow::Result<Vec<PeerInfo>> {
    let peer_infos: Vec<_> = state_chain_client
        .get_all_storage_pairs::<pallet_cf_validator::AccountPeerMapping::<state_chain_runtime::Runtime>>(block_hash)
        .await?
        .into_iter()
        .map(|(account_id, (public_key, port, ip_address))| {
            PeerInfo::new(account_id, public_key, ip_address.into(), port)
        })
        .collect();

    Ok(peer_infos)
}
