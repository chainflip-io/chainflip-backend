use std::sync::Arc;

use crate::{
	common::start_with_restart_on_failure,
	db::PersistentKeyDB,
	settings,
	state_chain_observer::client::StateChainClient,
	witnesser::{EpochStart, ItemMonitor, MonitorCommand},
};

use crate::state_chain_observer::client::storage_api::StorageApi;

use anyhow::{Context, Result};
use cf_chains::{
	dot::{PolkadotAccountId, PolkadotSignature},
	Polkadot,
};
use sp_core::H256;
use tokio::sync::Mutex;
use utilities::task_scope::Scope;

use crate::dot::witnesser;

use super::{rpc::DotRpcClient, runtime_version_updater};

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	dot_settings: &settings::Dot,
	epoch_start_receiver_1: async_broadcast::Receiver<EpochStart<Polkadot>>,
	epoch_start_receiver_2: async_broadcast::Receiver<EpochStart<Polkadot>>,
	initial_block_hash: H256,
	db: Arc<PersistentKeyDB>,
) -> Result<(
	tokio::sync::mpsc::UnboundedSender<MonitorCommand<PolkadotAccountId>>,
	tokio::sync::mpsc::UnboundedSender<MonitorCommand<PolkadotSignature>>,
)> {
	let (monitor_address_sender, address_monitor) = ItemMonitor::new(
		state_chain_client
			.storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
				state_chain_runtime::Runtime,
				state_chain_runtime::PolkadotInstance,
			>>(initial_block_hash)
			.await
			.context("Failed to get initial deposit details")?
			.into_iter()
			.filter_map(|(address, channel_details)| {
				if channel_details.source_asset == cf_primitives::chains::assets::dot::Asset::Dot {
					Some(address)
				} else {
					None
				}
			})
			.collect(),
	);

	let (monitor_signature_sender, signature_monitor) = ItemMonitor::new(
		state_chain_client
			.storage_map::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
				state_chain_runtime::Runtime,
				state_chain_runtime::PolkadotInstance,
			>>(initial_block_hash)
			.await
			.context("Failed to get initial DOT signatures to monitor")?
			.into_iter()
			.map(|(signature, _)| signature)
			.collect(),
	);

	let address_monitor = Arc::new(Mutex::new(address_monitor));
	let signature_monitor = Arc::new(Mutex::new(signature_monitor));

	let dot_settings_c = dot_settings.clone();
	let state_chain_client_c = state_chain_client.clone();
	let create_and_run_witnesser = move |resume_at_epoch: Option<EpochStart<Polkadot>>| {
		let dot_settings = dot_settings_c.clone();
		let epoch_start_receiver_1 = epoch_start_receiver_1.clone();
		let db = db.clone();
		let address_monitor = address_monitor.clone();
		let signature_monitor = signature_monitor.clone();
		let state_chain_client = state_chain_client_c.clone();
		async move {
			let dot_rpc_client =
				DotRpcClient::new(&dot_settings.ws_node_endpoint).await.map_err(|err| {
					tracing::error!("Failed to create DotRpcClient: {:?}", err);
				})?;
			witnesser::start(
				resume_at_epoch,
				epoch_start_receiver_1,
				dot_rpc_client,
				address_monitor,
				signature_monitor,
				state_chain_client,
				db,
			)
			.await
		}
	};

	scope.spawn(async move {
		start_with_restart_on_failure(create_and_run_witnesser).await;
		Ok(())
	});

	let dot_settings = dot_settings.clone();
	let create_and_run_runtime_version_updater =
		move |_resume_at_epoch: Option<EpochStart<Polkadot>>| {
			let dot_settings = dot_settings.clone();
			let epoch_start_receiver_2 = epoch_start_receiver_2.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let dot_rpc_client =
					DotRpcClient::new(&dot_settings.ws_node_endpoint).await.map_err(|err| {
						tracing::error!("Failed to create DotRpcClient: {:?}", err);
					})?;

				runtime_version_updater::start(
					epoch_start_receiver_2,
					dot_rpc_client,
					state_chain_client,
					initial_block_hash,
				)
				.await
			}
		};

	scope.spawn(async move {
		start_with_restart_on_failure(create_and_run_runtime_version_updater).await;
		Ok(())
	});

	Ok((monitor_address_sender, monitor_signature_sender))
}
