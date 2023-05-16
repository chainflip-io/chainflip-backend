use std::sync::Arc;

use crate::{
	db::PersistentKeyDB,
	settings,
	state_chain_observer::client::{storage_api::StorageApi, StateChainClient},
	witnesser::{AddressMonitor, AddressMonitorCommand, EpochStart, LatestBlockNumber},
};
use anyhow::{Context, Result};
use cf_chains::{btc::BitcoinScriptBounded, Bitcoin};
use futures::TryFutureExt;
use sp_core::H256;
use utilities::task_scope::Scope;

use super::rpc::BtcRpcClient;

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	btc_settings: &settings::Btc,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	initial_block_hash: H256,
	db: Arc<PersistentKeyDB>,
) -> Result<(
	tokio::sync::mpsc::UnboundedSender<AddressMonitorCommand<BitcoinScriptBounded>>,
	tokio::sync::mpsc::UnboundedSender<AddressMonitorCommand<[u8; 32]>>,
)> {
	let btc_rpc = BtcRpcClient::new(btc_settings)?;

	// We do a simple initial query here to test the connection. Else it's possible the connection
	// is bad but then we enter the witnesser loop which will retry until success.
	// Failing here means we will stop the engine.

	btc_rpc
		.latest_block_number()
		.await
		.context("Initial query for BTC latest block number failed.")?;

	let (address_monitor_command_sender, address_monitor) = AddressMonitor::new(
		state_chain_client
			.storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
				state_chain_runtime::Runtime,
				state_chain_runtime::BitcoinInstance,
			>>(initial_block_hash)
			.await
			.context("Failed to get initial BTC deposit details")?
			.into_iter()
			.filter_map(|(address, channel_details)| {
				if channel_details.source_asset == cf_primitives::chains::assets::btc::Asset::Btc {
					Some(address)
				} else {
					None
				}
			})
			.collect(),
	);
	// When we start how do we know what broadcasts to witness? We use the TransactionOutId storage
	// item on chain.

	let (tx_hash_monitor_sender, tx_hash_monitor) = AddressMonitor::new(Default::default());

	scope.spawn(
		super::witnesser::start(
			epoch_start_receiver,
			state_chain_client,
			btc_rpc,
			address_monitor,
			tx_hash_monitor,
			db,
		)
		.map_err(|_| anyhow::anyhow!("btc::witnesser::start failed")),
	);

	Ok((address_monitor_command_sender, tx_hash_monitor_sender))
}
