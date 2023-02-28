use std::{collections::BTreeSet, sync::Arc};

use anyhow::Result;
use cf_chains::Bitcoin;
use futures::TryFutureExt;

use crate::{multisig::PersistentKeyDB, settings, task_scope::Scope, witnesser::EpochStart};

use super::{rpc::BtcRpcClient, ScriptPubKey};

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	btc_settings: settings::Btc,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Bitcoin>>,
	db: Arc<PersistentKeyDB>,
) -> Result<tokio::sync::mpsc::UnboundedSender<ScriptPubKey>> {
	let (script_pubkeys_sender, script_pubkeys_receiver) = tokio::sync::mpsc::unbounded_channel();

	// TODO: query state chain for the script pubkeys to monitor

	scope.spawn(
		super::witnesser::start(
			epoch_start_receiver,
			BtcRpcClient::new(&btc_settings)?,
			script_pubkeys_receiver,
			BTreeSet::default(),
			db,
		)
		.map_err(|_| anyhow::anyhow!("btc::witnesser::start failed")),
	);

	Ok(script_pubkeys_sender)
}
