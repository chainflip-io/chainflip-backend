use std::sync::Arc;

use cf_primitives::EpochIndex;
use chainflip_engine::{
	dot::retry_rpc::DotRetryRpcClient,
	settings::NodeContainer,
	state_chain_observer::client::{
		storage_api::StorageApi,
		stream_api::{StreamApi, UNFINALIZED},
		StateChainClient, STATE_CHAIN_CONNECTION,
	},
	witness::{
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
		dot::{filter_map_events, process_egress, proxy_added_witnessing, DotUnfinalisedSource},
	},
};
use futures::Future;
use utilities::task_scope::Scope;

use crate::DepositTrackerSettings;

use super::EnvironmentParameters;

pub(super) async fn start<ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	witness_call: ProcessCall,
	settings: DepositTrackerSettings,
	env_params: EnvironmentParameters,
	state_chain_client: Arc<StateChainClient<()>>,
	state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient<()>, (), ()>,
) -> anyhow::Result<()>
where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let dot_client = DotRetryRpcClient::new(
		scope,
		NodeContainer { primary: settings.dot, backup: None },
		env_params.dot_genesis_hash,
	)?;

	let epoch_source = epoch_source
		.filter_map(
			|state_chain_client, _epoch_index, hash, _info| async move {
				state_chain_client
					.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
						hash,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
			},
			|_state_chain_client, _epoch, _block_hash, historic_info| async move { historic_info },
		)
		.await;

	let vaults = epoch_source.vaults().await;

	DotUnfinalisedSource::new(dot_client.clone())
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.strictly_monotonic()
		.chunk_by_vault(vaults.clone(), scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.dot_deposits(witness_call.clone())
		// Proxy added witnessing
		.then(proxy_added_witnessing)
		// Broadcast success
		.egress_items(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then(move |epoch, header| {
			process_egress(epoch, header, witness_call.clone(), dot_client.clone())
		})
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}
