use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use cf_chains::Bitcoin;
use cf_primitives::{BitcoinAddressSeed, ScriptPubkeyBytes};
use futures::TryFutureExt;

use crate::{
	multisig::PersistentKeyDB,
	settings,
	state_chain_observer::client::StateChainClient,
	task_scope::Scope,
	witnesser::{AddressMonitor, AddressMonitorCommand, EpochStart, LatestBlockNumber},
};

use super::rpc::BtcRpcClient;

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	btc_settings: &settings::Btc,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	db: Arc<PersistentKeyDB>,
) -> Result<
	tokio::sync::mpsc::UnboundedSender<
		AddressMonitorCommand<ScriptPubkeyBytes, BitcoinAddressSeed>,
	>,
> {
	let (script_pubkeys_sender, script_pubkeys_receiver) = tokio::sync::mpsc::unbounded_channel();

	let btc_rpc = BtcRpcClient::new(btc_settings)?;

	// We do a simple initial query here to test the connection. Else it's possible the connection
	// is bad but then we enter the witnesser loop which will retry until success.
	// Failing here means we will stop the engine.
	btc_rpc
		.latest_block_number()
		.await
		.context("Initial query for BTC latest block number")?;

	// TODO: query state chain for the script pubkeys to monitor and pass them into the witnesser

	scope.spawn(
		super::witnesser::start(
			epoch_start_receiver,
			state_chain_client,
			btc_rpc,
			AddressMonitor::new(BTreeMap::default(), script_pubkeys_receiver),
			db,
		)
		.map_err(|_| anyhow::anyhow!("btc::witnesser::start failed")),
	);

	Ok(script_pubkeys_sender)
}
