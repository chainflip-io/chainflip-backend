// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{
	collections::{BTreeSet, HashMap},
	net::Ipv6Addr,
	sync::Arc,
};

use anyhow::{bail, Context};
use cf_primitives::Port;
use engine_sc_client::{
	base_rpc_api::BaseRpcApi,
	chain_api::ChainApi,
	extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
	storage_api::StorageApi,
	StateChainClient,
};
use serde::de::DeserializeOwned;
use serde_json::{value::RawValue, Value};
use tokio::{
	sync::mpsc::UnboundedReceiver,
	time::{Duration, MissedTickBehavior},
};
use tracing::{error, info, warn};

const RESERVED_NEXT_AUTHORITIES_COUNT: usize = 2;
const RESERVED_PEER_RETRY_INTERVAL: Duration = Duration::from_secs(6);

#[derive(Clone, Debug)]
pub struct AuthoritiesUpdated {
	pub block_hash: state_chain_runtime::Hash,
	pub epoch_index: u32,
	pub authorities: Vec<state_chain_runtime::AccountId>,
}

pub async fn start(
	state_chain_client: Arc<StateChainClient>,
	mut authorities_update_receiver: UnboundedReceiver<AuthoritiesUpdated>,
) -> anyhow::Result<()> {
	ensure_local_node_p2p_address_registered(&state_chain_client)
		.await
		.context("Failed to ensure local Substrate node P2P address is registered on-chain")?;

	let mut controller = ReservedPeerController::new(state_chain_client);
	let initial_block_hash = controller.latest_finalized_block_hash();

	match controller.seed_pending_from_storage(initial_block_hash).await {
		Ok(()) => {
			if let Err(error) = controller.reconcile_pending_update(initial_block_hash).await {
				warn!(
					"Initial reserved-peer reconciliation failed: {error:#}. \
					Will retry shortly. \
					Ensure local state-chain RPC exposes `system_reservedPeers/system_addReservedPeer/system_removeReservedPeer`."
				);
			}
		},
		Err(error) => {
			warn!(
				"Failed to seed initial reserved-peer target from state-chain storage: {error:#}. \
				Will wait for authority-update CFE events."
			);
		},
	}

	let mut retry_tick = tokio::time::interval(RESERVED_PEER_RETRY_INTERVAL);
	retry_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

	loop {
		tokio::select! {
				authority_update = authorities_update_receiver.recv() => {
					match authority_update {
						Some(authority_update) => {
							let block_hash = authority_update.block_hash;
							let epoch_index = authority_update.epoch_index;
							controller.set_pending_update(authority_update);
							if let Err(error) = controller.reconcile_pending_update(block_hash).await {
								warn!(
									"Reserved-peer reconciliation failed for epoch {}: {error:#}",
									epoch_index
								);
							}
						},
					None => {
						error!("Reserved-peer controller stopped: authority-update channel closed");
						bail!("Reserved-peer controller stopped: authority-update channel closed")
					},
				}
			},
			_ = retry_tick.tick(), if controller.has_pending_update() => {
				if let Err(error) = controller.retry_pending_update_on_latest_block().await {
					warn!("Reserved-peer reconciliation retry failed: {error:#}");
				}
			},
		}
	}
}

/// Discover the local Substrate node's P2P address and ensure it is registered on-chain.
/// Submits a registration transaction only when the on-chain value differs.
async fn ensure_local_node_p2p_address_registered(
	state_chain_client: &Arc<StateChainClient>,
) -> anyhow::Result<()> {
	let (ip, port) = detect_substrate_p2p_address(state_chain_client)
		.await
		.context("Failed to auto-detect Substrate P2P address")?;

	let ipv6_addr = match ip {
		std::net::IpAddr::V4(v4) => v4.to_ipv6_mapped(),
		std::net::IpAddr::V6(v6) => v6,
	};

	let account_id = state_chain_client.account_id();
	let latest_finalized_block_hash = state_chain_client.latest_finalized_block().hash;
	let currently_registered = state_chain_client
		.storage_map_entry::<pallet_cf_validator::NodeP2pAddress<state_chain_runtime::Runtime>>(
			latest_finalized_block_hash,
			&account_id,
		)
		.await
		.context("Failed to read local node's currently registered Substrate P2P address")?;

	if currently_registered == Some((port, u128::from(ipv6_addr))) {
		info!(
			"Our Substrate node P2P address is already up to date on-chain: [{ipv6_addr}]:{port}"
		);
		return Ok(())
	}

	let extra_info = match currently_registered {
		Some((registered_port, registered_ip)) => {
			format!(
				"Node was previously registered with address [{}]:{registered_port}",
				Ipv6Addr::from(registered_ip)
			)
		},
		None => String::from("Node previously did not have a registered Substrate P2P address"),
	};
	info!(
		"Registering local Substrate node P2P address on-chain: [{ipv6_addr}]:{port}. {extra_info}."
	);

	state_chain_client
		.finalize_signed_extrinsic(pallet_cf_validator::Call::register_node_p2p_address {
			port,
			ip_address: u128::from(ipv6_addr),
		})
		.await
		.until_finalized()
		.await?;

	info!("Our Substrate node P2P address registration is now up to date!");
	Ok(())
}

/// Auto-detect the Substrate node's IP and TCP port from `system_localListenAddresses`.
/// Prefers non-loopback addresses; falls back to loopback if nothing else is available.
async fn detect_substrate_p2p_address(
	state_chain_client: &Arc<StateChainClient>,
) -> anyhow::Result<(std::net::IpAddr, Port)> {
	let raw = state_chain_client
		.base_rpc_client
		.request_raw("system_localListenAddresses", None)
		.await
		.context("RPC call failed for `system_localListenAddresses`")?;

	let listen_addrs: Vec<String> = serde_json::from_str(raw.get())
		.context("Failed to decode `system_localListenAddresses` response")?;

	// Prefer non-loopback TCP addresses.
	for addr in &listen_addrs {
		if let Some((ip, port)) = parse_tcp_address_from_multiaddr(addr) {
			if !ip.is_loopback() {
				return Ok((ip, port))
			}
		}
	}
	// Fall back to any TCP address (including loopback).
	for addr in &listen_addrs {
		if let Some(result) = parse_tcp_address_from_multiaddr(addr) {
			return Ok(result)
		}
	}

	bail!("No TCP listen address found in `system_localListenAddresses` response: {listen_addrs:?}")
}

/// Parse IP and TCP port from a multiaddr string like `/ip4/1.2.3.4/tcp/30333/p2p/12D3Koo...`.
/// Returns `None` if the multiaddr doesn't contain `/ip{4,6}/<addr>/tcp/<port>` or has a `/ws`
/// component (websocket addresses use a different protocol).
fn parse_tcp_address_from_multiaddr(multiaddr: &str) -> Option<(std::net::IpAddr, Port)> {
	if multiaddr.contains("/ws") {
		return None
	}
	let parts: Vec<&str> = multiaddr.split('/').collect();

	let mut ip: Option<std::net::IpAddr> = None;
	let mut port: Option<Port> = None;

	for window in parts.windows(2) {
		match window[0] {
			"ip4" => ip = window[1].parse::<std::net::Ipv4Addr>().ok().map(Into::into),
			"ip6" => ip = window[1].parse::<Ipv6Addr>().ok().map(Into::into),
			"tcp" => port = window[1].parse().ok(),
			_ => {},
		}
	}

	ip.zip(port)
}

// ── Reserved-peer controller ────────────────────────────────────────────────

/// Info needed per managed reserved peer: the PeerId (for removal) and the
/// full multiaddr (needed when we added it).
struct ReservedPeerController {
	state_chain_client: Arc<StateChainClient>,
	local_account_id: state_chain_runtime::AccountId,
	/// PeerIds that we have added as reserved peers and are responsible for removing.
	managed_peer_ids: BTreeSet<String>,
	pending_authorities_update: Option<PendingAuthoritiesUpdate>,
}

#[derive(Clone)]
struct PendingAuthoritiesUpdate {
	epoch_index: u32,
	authorities: Vec<state_chain_runtime::AccountId>,
}

impl ReservedPeerController {
	fn new(state_chain_client: Arc<StateChainClient>) -> Self {
		Self {
			local_account_id: state_chain_client.account_id(),
			state_chain_client,
			managed_peer_ids: BTreeSet::new(),
			pending_authorities_update: None,
		}
	}

	async fn seed_pending_from_storage(
		&mut self,
		block_hash: state_chain_runtime::Hash,
	) -> anyhow::Result<()> {
		let current_epoch = self.current_epoch(block_hash).await?;
		let authorities = self.current_authorities(block_hash).await?;

		self.pending_authorities_update =
			Some(PendingAuthoritiesUpdate { epoch_index: current_epoch, authorities });

		Ok(())
	}

	fn set_pending_update(&mut self, authority_update: AuthoritiesUpdated) {
		info!(
			"Received authority update for epoch {}; queued {} authorities for reserved-peer reconciliation.",
			authority_update.epoch_index,
			authority_update.authorities.len()
		);
		self.pending_authorities_update = Some(PendingAuthoritiesUpdate {
			epoch_index: authority_update.epoch_index,
			authorities: authority_update.authorities,
		});
	}

	fn has_pending_update(&self) -> bool {
		self.pending_authorities_update.is_some()
	}

	fn latest_finalized_block_hash(&self) -> state_chain_runtime::Hash {
		self.state_chain_client.latest_finalized_block().hash
	}

	async fn retry_pending_update_on_latest_block(&mut self) -> anyhow::Result<()> {
		let block_hash = self.latest_finalized_block_hash();
		self.reconcile_pending_update(block_hash).await
	}

	async fn reconcile_pending_update(
		&mut self,
		block_hash: state_chain_runtime::Hash,
	) -> anyhow::Result<()> {
		let Some(pending_update) = self.pending_authorities_update.clone() else { return Ok(()) };
		let authority_peer_ids = self.authority_peer_ids(block_hash).await?;
		let node_p2p_addresses = self.authority_node_p2p_addresses(block_hash).await?;

		let target_peers = target_reserved_peers(
			&self.local_account_id,
			&pending_update.authorities,
			&authority_peer_ids,
			&node_p2p_addresses,
			pending_update.epoch_index,
		);

		// `system_reservedPeers` returns PeerId strings.
		let existing_reserved_peer_ids = self.reserved_peers().await?;

		let target_peer_ids: BTreeSet<String> = target_peers.keys().cloned().collect();

		let mut had_reconcile_errors = false;

		// Add new reserved peers that aren't already reserved.
		for (peer_id, multiaddr) in &target_peers {
			if !existing_reserved_peer_ids.contains(peer_id) {
				match self.add_reserved_peer(multiaddr).await {
					Ok(()) => {
						info!(
							"Added temporary reserved peer for epoch {}: {multiaddr}",
							pending_update.epoch_index
						);
					},
					Err(error) => {
						had_reconcile_errors = true;
						warn!("Failed to add reserved peer `{multiaddr}`: {error:#}");
					},
				}
			}
		}

		// Remove stale peers that we previously managed but are no longer targets.
		let stale_peer_ids: Vec<_> =
			self.managed_peer_ids.difference(&target_peer_ids).cloned().collect();
		for peer_id in stale_peer_ids {
			match self.remove_reserved_peer(&peer_id).await {
				Ok(()) => {
					info!("Removed temporary reserved peer from previous epoch: {peer_id}");
				},
				Err(error) => {
					had_reconcile_errors = true;
					warn!("Failed to remove reserved peer `{peer_id}`: {error:#}");
				},
			}
		}

		if had_reconcile_errors {
			bail!(
				"Reserved-peer reconciliation was incomplete for epoch {}",
				pending_update.epoch_index
			);
		}

		self.managed_peer_ids = target_peer_ids;
		self.pending_authorities_update = None;
		Ok(())
	}

	async fn current_epoch(&self, block_hash: state_chain_runtime::Hash) -> anyhow::Result<u32> {
		self.state_chain_client
			.storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>(
				block_hash,
			)
			.await
			.context("Failed to read current epoch from storage")
	}

	async fn current_authorities(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> anyhow::Result<Vec<state_chain_runtime::AccountId>> {
		self.state_chain_client
			.storage_value::<pallet_cf_validator::CurrentAuthorities<state_chain_runtime::Runtime>>(
				block_hash,
			)
			.await
			.context("Failed to read current authorities from storage")
	}

	async fn authority_peer_ids(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> anyhow::Result<HashMap<state_chain_runtime::AccountId, String>> {
		let peer_mappings: Vec<_> = self
			.state_chain_client
			.storage_map::<pallet_cf_validator::AccountPeerMapping<state_chain_runtime::Runtime>, Vec<_>>(
				block_hash,
			)
			.await
			.context("Failed to read authority peer mappings from storage")?;

		peer_mappings
			.into_iter()
			.map(|(account_id, (peer_public_key, _, _))| {
				registered_peer_id_from_ed25519_key(peer_public_key)
					.map(|peer_id| (account_id, peer_id))
			})
			.collect()
	}

	async fn authority_node_p2p_addresses(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> anyhow::Result<HashMap<state_chain_runtime::AccountId, (Port, Ipv6Addr)>> {
		let addresses: Vec<_> = self
			.state_chain_client
			.storage_map::<pallet_cf_validator::NodeP2pAddress<state_chain_runtime::Runtime>, Vec<_>>(
				block_hash,
			)
			.await
			.context("Failed to read node P2P addresses from storage")?;

		Ok(addresses
			.into_iter()
			.map(|(account_id, (port, ip))| (account_id, (port, Ipv6Addr::from(ip))))
			.collect())
	}

	/// Returns the set of currently reserved PeerIds.
	async fn reserved_peers(&self) -> anyhow::Result<BTreeSet<String>> {
		let peers: Vec<String> = self.request_json("system_reservedPeers", &[]).await?;
		Ok(peers.into_iter().collect())
	}

	/// `system_addReservedPeer` expects a full multiaddr string.
	async fn add_reserved_peer(&self, multiaddr: &str) -> anyhow::Result<()> {
		self.request_json("system_addReservedPeer", &[Value::String(multiaddr.to_string())])
			.await
	}

	/// `system_removeReservedPeer` expects a PeerId string.
	async fn remove_reserved_peer(&self, peer_id: &str) -> anyhow::Result<()> {
		self.request_json("system_removeReservedPeer", &[Value::String(peer_id.to_string())])
			.await
	}

	async fn request_json<Response: DeserializeOwned>(
		&self,
		method: &str,
		params: &[Value],
	) -> anyhow::Result<Response> {
		let raw = self
			.state_chain_client
			.base_rpc_client
			.request_raw(method, to_raw_rpc_params(params)?)
			.await
			.with_context(|| format!("RPC call failed for `{method}`"))?;

		serde_json::from_str(raw.get())
			.with_context(|| format!("Failed to decode response for `{method}`"))
	}
}

fn registered_peer_id_from_ed25519_key(
	peer_public_key: sp_core::ed25519::Public,
) -> anyhow::Result<String> {
	let libp2p_public =
		libp2p_identity::ed25519::PublicKey::try_from_bytes(peer_public_key.as_ref())
			.context("Failed to parse on-chain ed25519 peer key")?;

	Ok(libp2p_identity::PublicKey::from(libp2p_public).to_peer_id().to_string())
}

/// Build a multiaddr from an IPv6 address, TCP port, and PeerId.
fn build_multiaddr(ip: &Ipv6Addr, port: Port, peer_id: &str) -> String {
	format!("/ip6/{ip}/tcp/{port}/p2p/{peer_id}")
}

/// Determine which peers should be reserved for the current epoch.
/// Returns a map of PeerId → multiaddr for the next N authorities.
fn target_reserved_peers(
	local_account_id: &state_chain_runtime::AccountId,
	authorities: &[state_chain_runtime::AccountId],
	authority_peer_ids: &HashMap<state_chain_runtime::AccountId, String>,
	node_p2p_addresses: &HashMap<state_chain_runtime::AccountId, (Port, Ipv6Addr)>,
	current_epoch: u32,
) -> HashMap<String, String> {
	let Some(local_index) = authorities.iter().position(|authority| authority == local_account_id)
	else {
		info!(
			"Local validator `{}` is not part of current authority set at epoch {}. No temporary reserved peers will be managed.",
			local_account_id,
			current_epoch
		);
		return HashMap::new()
	};

	let max_next = RESERVED_NEXT_AUTHORITIES_COUNT.min(authorities.len().saturating_sub(1));
	let mut targets = HashMap::new();

	for offset in 1..=max_next {
		let next_authority = &authorities[(local_index + offset) % authorities.len()];

		let Some(peer_id) = authority_peer_ids.get(next_authority) else {
			warn!(
				"No on-chain peer-id registration found for next authority `{}` at epoch {}",
				next_authority, current_epoch
			);
			continue
		};

		let Some((port, ip)) = node_p2p_addresses.get(next_authority) else {
			warn!(
				"No on-chain Substrate P2P address for next authority `{}` at epoch {}. \
				They may not have registered their node address yet.",
				next_authority, current_epoch
			);
			continue
		};

		let multiaddr = build_multiaddr(ip, *port, peer_id);
		targets.insert(peer_id.clone(), multiaddr);
	}

	targets
}

fn to_raw_rpc_params(params: &[Value]) -> anyhow::Result<Option<Box<RawValue>>> {
	if params.is_empty() {
		return Ok(None)
	}

	let encoded = serde_json::to_string(params).context("Failed to encode RPC params")?;
	Ok(Some(RawValue::from_string(encoded).context("Failed to convert RPC params to raw JSON")?))
}

#[cfg(test)]
mod tests {
	use std::{collections::HashMap, net::Ipv6Addr};

	use super::{build_multiaddr, parse_tcp_address_from_multiaddr, target_reserved_peers};

	#[test]
	fn includes_next_authorities_with_wraparound() {
		let local = state_chain_runtime::AccountId::new([1; 32]);
		let a2 = state_chain_runtime::AccountId::new([2; 32]);
		let a3 = state_chain_runtime::AccountId::new([3; 32]);

		let authorities = vec![a2.clone(), a3.clone(), local.clone()];
		let authority_peer_ids =
			HashMap::from([(a2.clone(), "peer2".to_string()), (a3.clone(), "peer3".to_string())]);

		let ip2 = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc0a8, 0x0002);
		let ip3 = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc0a8, 0x0003);
		let node_p2p_addresses = HashMap::from([(a2, (30333u16, ip2)), (a3, (30333u16, ip3))]);

		let targets = target_reserved_peers(
			&local,
			&authorities,
			&authority_peer_ids,
			&node_p2p_addresses,
			10,
		);

		assert_eq!(targets.len(), 2);
		assert_eq!(targets.get("peer2").unwrap(), &build_multiaddr(&ip2, 30333, "peer2"));
		assert_eq!(targets.get("peer3").unwrap(), &build_multiaddr(&ip3, 30333, "peer3"));
	}

	#[test]
	fn ignores_missing_mappings() {
		let local = state_chain_runtime::AccountId::new([1; 32]);
		let a2 = state_chain_runtime::AccountId::new([2; 32]);
		let authorities = vec![local.clone(), a2];
		let authority_peer_ids = HashMap::new();
		let node_p2p_addresses = HashMap::new();

		let targets = target_reserved_peers(
			&local,
			&authorities,
			&authority_peer_ids,
			&node_p2p_addresses,
			2,
		);

		assert!(targets.is_empty());
	}

	#[test]
	fn ignores_authority_without_node_p2p_address() {
		let local = state_chain_runtime::AccountId::new([1; 32]);
		let a2 = state_chain_runtime::AccountId::new([2; 32]);
		let authorities = vec![local.clone(), a2.clone()];
		let authority_peer_ids = HashMap::from([(a2, "peer2".to_string())]);
		// No node P2P address registered for a2.
		let node_p2p_addresses = HashMap::new();

		let targets = target_reserved_peers(
			&local,
			&authorities,
			&authority_peer_ids,
			&node_p2p_addresses,
			5,
		);

		assert!(targets.is_empty());
	}

	#[test]
	fn parse_tcp_address_from_multiaddr_works() {
		use std::net::IpAddr;

		assert_eq!(
			parse_tcp_address_from_multiaddr("/ip4/198.51.100.19/tcp/30333/p2p/QmSk5"),
			Some(("198.51.100.19".parse::<IpAddr>().unwrap(), 30333))
		);
		assert_eq!(
			parse_tcp_address_from_multiaddr("/ip6/::1/tcp/30334/p2p/QmSk5"),
			Some(("::1".parse::<IpAddr>().unwrap(), 30334))
		);
		// Websocket addresses should be skipped.
		assert_eq!(parse_tcp_address_from_multiaddr("/ip4/127.0.0.1/tcp/30334/ws/p2p/QmSk5"), None);
		// No TCP segment.
		assert_eq!(parse_tcp_address_from_multiaddr("/ip4/127.0.0.1/udp/30333"), None);
		// No IP segment.
		assert_eq!(parse_tcp_address_from_multiaddr("/tcp/30333"), None);
	}

	#[test]
	fn build_multiaddr_formats_correctly() {
		let ip = Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc633, 0x6413);
		let result = build_multiaddr(&ip, 30333, "12D3KooWFoo");
		assert_eq!(result, "/ip6/::ffff:198.51.100.19/tcp/30333/p2p/12D3KooWFoo");
	}
}
