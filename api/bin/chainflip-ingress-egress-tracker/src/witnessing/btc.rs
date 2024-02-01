use std::sync::Arc;

use cf_primitives::EpochIndex;
use chainflip_engine::{
	btc::retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
	settings::NodeContainer,
	state_chain_observer::client::{
		stream_api::{StreamApi, UNFINALIZED},
		StateChainClient,
	},
	witness::{
		btc::{btc_source::BtcSource, process_egress},
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
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
	let btc_client = BtcRetryRpcClient::new(
		scope,
		NodeContainer { primary: settings.btc, backup: None },
		env_params.chainflip_network.into(),
	)
	.await?;

	let vaults = epoch_source.vaults().await;

	BtcSource::new(btc_client.clone())
		.strictly_monotonic()
		.then({
			let btc_client = btc_client.clone();
			move |header| {
				let btc_client = btc_client.clone();
				async move {
					let block = btc_client.block(header.hash).await;
					(header.data, block.txdata)
				}
			}
		})
		.chunk_by_vault(vaults, scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.btc_deposits(witness_call.clone())
		.egress_items(scope, state_chain_stream, state_chain_client)
		.await
		.then(move |epoch, header| process_egress(epoch, header, witness_call.clone()))
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}
