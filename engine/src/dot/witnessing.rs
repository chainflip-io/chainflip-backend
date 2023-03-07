use std::sync::Arc;

use super::{rpc::DotRpcClient, witnesser::WitnesserReceiverPairs};
use crate::{
	common::start_with_restart_on_failure,
	multisig::PersistentKeyDB,
	settings,
	state_chain_observer::client::{storage_api::StorageApi, StateChainClient},
	task_scope::Scope,
	try_with_logging,
	witnesser::EpochStart,
};

use anyhow::{Context, Result};
use cf_chains::Polkadot;
use sp_runtime::AccountId32;

pub struct DotMonitorSenders {
	pub ingress: tokio::sync::mpsc::UnboundedSender<AccountId32>,
	pub signature: tokio::sync::mpsc::UnboundedSender<[u8; 64]>,
}

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	dot_settings: settings::Dot,
	state_chain_client: Arc<StateChainClient>,
	latest_block_hash: sp_core::H256,
	epoch_start_receiver_1: async_broadcast::Receiver<EpochStart<Polkadot>>,
	epoch_start_receiver_2: async_broadcast::Receiver<EpochStart<Polkadot>>,
	db: Arc<PersistentKeyDB>,
) -> Result<DotMonitorSenders> {
	let (ingress_sender, ingress_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (signature_sender, signature_receiver) = tokio::sync::mpsc::unbounded_channel();

	let initial_ingress_addresses_to_monitor = state_chain_client
		.storage_map::<pallet_cf_ingress_egress::IntentIngressDetails<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>(latest_block_hash)
		.await
		.context("Failed to get initial ingress details")?
		.into_iter()
		.filter_map(|(address, intent)| {
			if intent.ingress_asset == cf_primitives::chains::assets::dot::Asset::Dot {
				Some(address)
			} else {
				None
			}
		})
		.collect();

	// NB: We don't need to monitor Ethereum signatures because we already monitor
	// signature accepted events from the KeyManager contract on Ethereum.
	let initial_signatures_to_monitor = state_chain_client
		.storage_map::<pallet_cf_broadcast::SignatureToBroadcastIdLookup<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>(latest_block_hash)
		.await
		.context("Failed to get initial DOT signatures to monitor")?
		.into_iter()
		.map(|(signature, _)| signature.0)
		.collect();

	let state_chain_client_c = state_chain_client.clone();
	let dot_settings_c = dot_settings.clone();
	let create_and_run_witnesser_future =
		move |(epoch_start_receiver, witnesser_receiver_pairs)| {
			let dot_settings = dot_settings_c.clone();
			let state_chain_client = state_chain_client_c.clone();
			let db = db.clone();
			async move {
				let dot_rpc_client = try_with_logging!(
					DotRpcClient::new(&dot_settings.ws_node_endpoint).await,
					(epoch_start_receiver, witnesser_receiver_pairs)
				);

				super::witnesser::start(
					epoch_start_receiver,
					dot_rpc_client.clone(),
					witnesser_receiver_pairs,
					state_chain_client.clone(),
					db,
				)
				.await
			}
		};

	scope.spawn(async move {
		start_with_restart_on_failure(
			create_and_run_witnesser_future,
			(
				epoch_start_receiver_1,
				WitnesserReceiverPairs {
					ingress: (ingress_receiver, initial_ingress_addresses_to_monitor),
					signature: (signature_receiver, initial_signatures_to_monitor),
				},
			),
		)
		.await;

		Ok(())
	});

	// When this witnesser starts up, we should check that the runtime version is up to
	// date with the chain. This is in case we missed a Polkadot runtime upgrade when
	// we were down.
	let on_chain_runtime_version = state_chain_client
		.storage_value::<pallet_cf_environment::PolkadotRuntimeVersion<state_chain_runtime::Runtime>>(
			latest_block_hash,
		)
		.await
		.context("Failed to get the registered Polkadot version from the SC")?;

	let create_and_run_update_future = move |(epoch_start_receiver, last_version_witnessed)| {
		let dot_settings = dot_settings.clone();
		let state_chain_client = state_chain_client.clone();
		async move {
			let dot_rpc_client = try_with_logging!(
				DotRpcClient::new(&dot_settings.ws_node_endpoint).await,
				(epoch_start_receiver, last_version_witnessed)
			);
			super::runtime_version_updater::start(
				epoch_start_receiver,
				dot_rpc_client,
				last_version_witnessed,
				state_chain_client,
			)
			.await
		}
	};

	scope.spawn(async move {
		start_with_restart_on_failure(
			create_and_run_update_future,
			(epoch_start_receiver_2, on_chain_runtime_version),
		)
		.await;

		Ok(())
	});

	Ok(DotMonitorSenders { ingress: ingress_sender, signature: signature_sender })
}
