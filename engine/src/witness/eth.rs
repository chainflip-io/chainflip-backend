use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	eth::{ethers_rpc::EthersRpcClient, retry_rpc::EthersRetryRpcClient},
	settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use super::{
	chain_source::{eth_source::EthSource, extension::ChainSourceExt},
	common::STATE_CHAIN_CONNECTION,
	epoch_source::EpochSource,
};

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let expected_chain_id = web3::types::U256::from(
		state_chain_client
			.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
				state_chain_stream.cache().block_hash,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION),
	);

	let eth_client = EthersRetryRpcClient::new(
		scope,
		EthersRpcClient::new(settings).await.unwrap(),
		settings.ws_node_endpoint.clone(),
		expected_chain_id,
	);

	let eth_source = EthSource::new(eth_client.clone());

	let epoch_source = EpochSource::new(scope, state_chain_stream, state_chain_client.clone())
		.await
		.participating(state_chain_client.account_id())
		.await;

	let eth_chain_tracking = eth_source
		.shared(scope)
		.chunk_by_time(epoch_source)
		.await
		.chain_tracking(state_chain_client, eth_client)
		.run();

	scope.spawn(async move {
		eth_chain_tracking.await;
		Ok(())
	});
}
