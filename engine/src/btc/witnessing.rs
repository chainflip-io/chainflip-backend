use std::sync::Arc;

use crate::{
	multisig::PersistentKeyDB,
	settings,
	state_chain_observer::client::{storage_api::StorageApi, StateChainClient},
	task_scope::Scope,
	witnesser::{AddressMonitor, AddressMonitorCommand, EpochStart},
};
use anyhow::{Context, Result};
use cf_chains::{address::BitcoinAddressData, Bitcoin};
use futures::TryFutureExt;
use sp_core::H256;

use super::rpc::BtcRpcClient;

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	btc_settings: &settings::Btc,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	initial_block_hash: H256,
	db: Arc<PersistentKeyDB>,
) -> Result<tokio::sync::mpsc::UnboundedSender<AddressMonitorCommand<BitcoinAddressData>>> {
	let btc_rpc = BtcRpcClient::new(btc_settings)?;

	// We do a simple initial query here to test the connection. Else it's possible the connection
	// is bad but then we enter the witnesser loop which will retry until success.
	// Failing here means we will stop the engine.

	// TODO: Re-instate this once test-single-node is fixed.
	// btc_rpc
	// 	.latest_block_number()
	// 	.await
	// 	.context("Initial query for BTC latest block number failed.")?;

	let (ingress_sender, address_monitor) = AddressMonitor::new(
		state_chain_client
			.storage_map::<pallet_cf_ingress_egress::IntentIngressDetails<
				state_chain_runtime::Runtime,
				state_chain_runtime::BitcoinInstance,
			>>(initial_block_hash)
			.await
			.context("Failed to get initial BTC ingress details")?
			.into_iter()
			.filter_map(|(address, intent)| {
				if intent.ingress_asset == cf_primitives::chains::assets::btc::Asset::Btc {
					Some(address)
				} else {
					None
				}
			})
			.collect(),
	);

	scope.spawn(
		super::witnesser::start(
			epoch_start_receiver,
			state_chain_client,
			btc_rpc,
			address_monitor,
			db,
		)
		.map_err(|_| anyhow::anyhow!("btc::witnesser::start failed")),
	);

	Ok(ingress_sender)
}
