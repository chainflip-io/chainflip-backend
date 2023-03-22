use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result};
use cf_chains::Bitcoin;
use cf_primitives::BitcoinAddress;
use futures::TryFutureExt;

use crate::{
	multisig::PersistentKeyDB,
	settings,
	task_scope::Scope,
	witnesser::{EpochStart, LatestBlockNumber},
};

use super::rpc::BtcRpcClient;

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	btc_settings: settings::Btc,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	db: Arc<PersistentKeyDB>,
) -> Result<tokio::sync::mpsc::UnboundedSender<BitcoinAddress>> {
	let (script_pubkeys_sender, script_pubkeys_receiver) = tokio::sync::mpsc::unbounded_channel();

	let btc_rpc = BtcRpcClient::new(&btc_settings)?;

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
			btc_rpc,
			script_pubkeys_receiver,
			BTreeSet::default(),
			db,
		)
		.map_err(|_| anyhow::anyhow!("btc::witnesser::start failed")),
	);

	Ok(script_pubkeys_sender)
}
