use std::sync::Arc;

use crate::{
	common::start_with_restart_on_failure, settings,
	state_chain_observer::client::StateChainClient, witnesser::EpochStart,
};

use anyhow::Result;
use cf_chains::Polkadot;
use sp_core::H256;
use utilities::task_scope::Scope;

use super::{http_rpc::DotHttpRpcClient, rpc::DotRpcClient, runtime_version_updater};

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient>,
	dot_settings: &settings::Dot,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	initial_block_hash: H256,
) -> Result<()> {
	let dot_settings = dot_settings.clone();
	let create_and_run_runtime_version_updater =
		move |_resume_at_epoch: Option<EpochStart<Polkadot>>| {
			let dot_settings = dot_settings.clone();
			let epoch_start_receiver = epoch_start_receiver.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let dot_rpc_client = DotRpcClient::new(
					&dot_settings.ws_node_endpoint,
					DotHttpRpcClient::new(&dot_settings.http_node_endpoint).await.map_err(|e| {
						tracing::error!("Dot HTTP RPC Client failed to be initialised: {e:?}");
					})?,
				)
				.await
				.map_err(|err| {
					tracing::error!("Failed to create DotRpcClient: {:?}", err);
				})?;

				runtime_version_updater::start(
					epoch_start_receiver,
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

	Ok(())
}
