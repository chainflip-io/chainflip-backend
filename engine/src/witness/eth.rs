use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	eth::{ethers_rpc::EthersRpcClient, retry_rpc::EthersRetryRpcClient},
	settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};

use super::{
	chain_source::{eth_source::EthSource, extension::ChainSourceExt},
	common::STATE_CHAIN_CONNECTION,
	epoch_source::EpochSource,
};

use anyhow::Result;

pub async fn start<StateChainClient, Epochs: Into<EpochSource<(), ()>>>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	epoch_source: Epochs,
	initial_block_hash: state_chain_runtime::Hash,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let expected_chain_id = web3::types::U256::from(
		state_chain_client
			.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
				initial_block_hash,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION),
	);

	let eth_client = EthersRetryRpcClient::new(
		scope,
		EthersRpcClient::new(settings).await?,
		settings.ws_node_endpoint.clone(),
		expected_chain_id,
	);

	let eth_chain_tracking = EthSource::new(eth_client.clone())
		.shared(scope)
		.chunk_by_time(epoch_source)
		.chain_tracking(state_chain_client, eth_client)
		.run();

	scope.spawn(async move {
		eth_chain_tracking.await;
		Ok(())
	});

	Ok(())
}
