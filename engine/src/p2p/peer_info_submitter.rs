use std::{
	net::{IpAddr, Ipv6Addr},
	sync::Arc,
};

use anyhow::Result;

use codec::Encode;
use sp_core::H256;
use tracing::info;
use utilities::Port;

use crate::{
	p2p::PeerInfo,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
		storage_api::StorageApi,
	},
};

use super::P2PKey;

async fn update_registered_peer_id<StateChainClient>(
	p2p_key: &P2PKey,
	state_chain_client: &Arc<StateChainClient>,
	ip_address: Ipv6Addr,
	cfe_port: Port,
) -> Result<()>
where
	StateChainClient: SignedExtrinsicApi,
{
	let peer_id = sp_core::ed25519::Public(p2p_key.signing_key.verifying_key().to_bytes());

	let signature = {
		use ed25519_dalek::Signer;
		let payload = &state_chain_client.account_id().encode();
		p2p_key.signing_key.sign(payload)
	};

	state_chain_client
		.finalize_signed_extrinsic(pallet_cf_validator::Call::register_peer_id {
			peer_id,
			port: cfe_port,
			ip_address: ip_address.into(),
			// We sign over our account id
			signature: sp_core::ed25519::Signature(signature.to_bytes()),
		})
		.await
		.until_finalized()
		.await?;

	Ok(())
}

pub(super) async fn ensure_peer_info_registered<StateChainClient>(
	p2p_key: &P2PKey,
	state_chain_client: &Arc<StateChainClient>,
	ip_address: IpAddr,
	cfe_port: Port,
	previous_registered_peer_info: Option<PeerInfo>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + ChainApi + Send + Sync,
{
	let ip_address = match ip_address {
		IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
		IpAddr::V6(ipv6) => ipv6,
	};

	let public_encryption_key = p2p_key.encryption_key.public_key;

	if Some((ip_address, cfe_port, public_encryption_key)) !=
		previous_registered_peer_info.as_ref().map(|pi| (pi.ip, pi.port, pi.pubkey))
	{
		let extra_info = match previous_registered_peer_info.as_ref() {
			Some(peer_info) => {
				format!(
					"Node was previously registered with address [{}]:{}",
					peer_info.ip, peer_info.port
				)
			},
			None => String::from("Node previously did not have a registered address"),
		};

		info!(
		"Registering node's peer info. Address: [{ip_address}]:{cfe_port}, x25519 public key: {}. {extra_info}.",
	super::pk_to_string(&public_encryption_key));

		update_registered_peer_id(p2p_key, state_chain_client, ip_address, cfe_port).await?;
		info!("Our peer info registration is now up to date!");
	} else {
		info!("Our peer info registration is already up to date!");
	}

	Ok(())
}

pub async fn get_current_peer_infos<StateChainClient>(
	state_chain_client: &Arc<StateChainClient>,
	block_hash: H256,
) -> anyhow::Result<Vec<PeerInfo>>
where
	StateChainClient: StorageApi + ChainApi,
{
	let peer_infos: Vec<_> = state_chain_client
		.storage_map::<pallet_cf_validator::AccountPeerMapping<state_chain_runtime::Runtime>, Vec<_>>(
			block_hash,
		)
		.await?
		.into_iter()
		.map(|(account_id, (public_key, port, ip_address))| {
			PeerInfo::new(account_id, public_key, ip_address.into(), port)
		})
		.collect();

	Ok(peer_infos)
}
