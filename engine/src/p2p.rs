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

pub mod multisig_adapter;
mod peer_info_submitter;
#[cfg(test)]
mod tests;

use std::{
	net::{IpAddr, Ipv4Addr},
	sync::Arc,
};

use crate::settings::P2P as P2PSettings;

use engine_sc_client::{
	chain_api::ChainApi,
	extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi,
	stream_api::{StreamApi, FINALIZED},
};

pub use engine_p2p::{PeerInfo, PeerUpdate};

pub use multisig_adapter::{MultisigChannels, MultisigMessageReceiver, MultisigMessageSender};

use anyhow::Context;
use futures::{Future, FutureExt, StreamExt};
use sp_core::H256;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::{error, info, info_span, Instrument};
use zeroize::Zeroizing;

use cf_utilities::{read_clean_and_decode_hex_str_file, task_scope::task_scope};

pub async fn start<StateChainClient, BlockStream: StreamApi<FINALIZED>>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	settings: P2PSettings,
	initial_block_hash: H256,
) -> anyhow::Result<(
	MultisigChannels,
	oneshot::Receiver<()>,
	impl Future<Output = anyhow::Result<()>>,
)>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + ChainApi + 'static + Send + Sync,
{
	if settings.ip_address == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
		anyhow::bail!("Should provide a valid IP address");
	}

	if !settings.allow_local_ip && !IpAddr::is_global(&settings.ip_address) {
		anyhow::bail!("Provided IP address is not globally routable");
	}

	let node_key = {
		let mut ed_secret_key = Zeroizing::new(ed25519_dalek::SecretKey::default());
		read_clean_and_decode_hex_str_file(&settings.node_key_file, "Node Key", |str| {
			hex::decode_to_slice(str, &mut ed_secret_key[..]).map_err(anyhow::Error::msg)
		})?;

		engine_p2p::P2PKey::new(&ed_secret_key)
	};

	let current_peers =
		peer_info_submitter::get_current_peer_infos(&state_chain_client, initial_block_hash)
			.await
			.context("Failed to get initial peer info")?;
	let our_account_id = state_chain_client.account_id();

	let own_peer_info = current_peers.iter().find(|pi| pi.account_id == our_account_id).cloned();

	let (incoming_message_sender, incoming_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (outgoing_message_sender, outgoing_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

	// Carries the newly-selected transport from the monitor to the supervisor when governance
	// changes the network-wide transport, so it can be restarted in-process.
	let (transport_restart_sender, transport_restart_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (p2p_ready_sender, p2p_ready_receiver) = oneshot::channel();

	// Create multisig channels with topic muxer
	let (multisig_channels, muxer_future) =
		MultisigChannels::new(incoming_message_receiver, outgoing_message_sender);

	// Read the network-wide P2P transport selection from the State Chain. If it later
	// changes, the monitor task below shuts the engine down so it restarts on the new
	// transport, keeping the whole validator set on a single (wire-incompatible) transport.
	let initial_transport = state_chain_client
		.storage_value::<pallet_cf_environment::P2pTransportValue<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to read the P2P transport setting from the State Chain")?;
	info!("P2P transport selected by the State Chain: {initial_transport}");

	let fut = task_scope(move |scope| {
		async move {
			scope.spawn({
				let state_chain_client = state_chain_client.clone();
				async move {
					peer_info_submitter::ensure_peer_info_registered(
						&node_key,
						&state_chain_client,
						settings.ip_address,
						settings.port,
						own_peer_info,
					)
					.instrument(info_span!("P2PClient"))
					.await?;

					p2p_ready_sender.send(()).unwrap();

					// The supervisor owns the muxer-facing channels and (re)starts the
					// transport in-process whenever the monitor signals a change, so a
					// transport switch does not disturb the muxer or the multisig ceremonies
					// above it.
					engine_p2p::supervisor::run_transport_supervisor(
						to_engine_transport(initial_transport),
						node_key,
						settings.port,
						our_account_id,
						current_peers,
						incoming_message_sender,
						outgoing_message_receiver,
						peer_update_receiver,
						transport_restart_receiver,
					)
					.await?;

					Ok(())
				}
			});

			scope.spawn(async move {
				muxer_future.await;
				Ok(())
			});

			scope.spawn(async move {
				monitor_p2p_registration_events(
					state_chain_client,
					sc_block_stream,
					peer_update_sender,
					transport_restart_sender,
					initial_transport,
				)
				.await
			});

			Ok(())
		}
		.boxed()
	});

	Ok((multisig_channels, p2p_ready_receiver, fut))
}

/// Maps the on-chain transport selection to the engine's transport enum.
fn to_engine_transport(transport: cf_primitives::P2pTransport) -> engine_p2p::Transport {
	match transport {
		cf_primitives::P2pTransport::Zmq => engine_p2p::Transport::Zmq,
		cf_primitives::P2pTransport::Quic => engine_p2p::Transport::Quic,
	}
}

/// Monitors the State Chain for peer registration events and sends them to the P2P client.
/// This is done separate to the SC Observer because we do not want to process events in the initial
/// block.
async fn monitor_p2p_registration_events<StateChainClient, BlockStream: StreamApi<FINALIZED>>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	peer_update_sender: UnboundedSender<PeerUpdate>,
	transport_restart_sender: UnboundedSender<engine_p2p::Transport>,
	initial_transport: cf_primitives::P2pTransport,
) -> anyhow::Result<()>
where
	StateChainClient: StorageApi + 'static + Send + Sync,
{
	use state_chain_runtime::Runtime;
	type CfeEvent = pallet_cf_cfe_interface::CfeEvent<Runtime>;

	let mut current_transport = initial_transport;
	let mut sc_block_stream = Box::pin(sc_block_stream);
	loop {
		match sc_block_stream.next().await {
			Some(current_block) => {
				if let Ok(events) = state_chain_client
					.storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>(
						current_block.hash,
					)
					.await
				{
					for event in events {
						match event {
							CfeEvent::PeerIdRegistered { account_id, pubkey, port, ip } => {
								peer_update_sender
									.send(PeerUpdate::Registered(PeerInfo::new(
										account_id,
										pubkey,
										ip.into(),
										port,
									)))
									.unwrap();
							},
							CfeEvent::PeerIdDeregistered { account_id, pubkey } => {
								peer_update_sender
									.send(PeerUpdate::Deregistered(account_id, pubkey))
									.unwrap();
							},
							_ => {
								// We only care about peer registration events
							},
						}
					}
				}

				// If governance has changed the network-wide transport, ask the supervisor to
				// restart the transport in-process on the new selection. We only act on a
				// confirmed change; a transient read error leaves the current transport running.
				if let Ok(on_chain_transport) = state_chain_client
					.storage_value::<pallet_cf_environment::P2pTransportValue<Runtime>>(
						current_block.hash,
					)
					.await
				{
					if on_chain_transport != current_transport {
						info!(
							"P2P transport changed from {current_transport} to {on_chain_transport}; \
							 restarting the transport in-process"
						);
						// If the supervisor has gone away the engine is already shutting down,
						// so a send error here is benign.
						let _ = transport_restart_sender
							.send(to_engine_transport(on_chain_transport));
						current_transport = on_chain_transport;
					}
				}
			},
			None => {
				error!("Exiting as State Chain block stream ended");
				break
			},
		}
	}
	Ok(())
}
