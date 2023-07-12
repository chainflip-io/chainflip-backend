use std::sync::Arc;

use cf_chains::eth::EthereumTrackedData;
use futures_util::FutureExt;
use sp_core::U256;
use utilities::{context, task_scope};

use crate::{
	eth::{
		ethers_rpc::EthersRpcClient,
		retry_rpc::{EthersRetryRpcApi, EthersRetryRpcClient},
	},
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use super::{
	chain_source::{eth_source::EthSource, extension::ChainSourceExt},
	chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
	epoch_source::EpochSource,
};

#[async_trait::async_trait]
impl<T: EthersRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Ethereum> for T {
	async fn get_tracked_data(
		&self,
		block_number: <cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber,
	) -> Result<<cf_chains::Ethereum as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let priority_fee_percentile = 50u8;
		let fee_history = self
			.fee_history(
				U256::one(),
				block_number.into(),
				vec![priority_fee_percentile as f64 / 100_f64],
			)
			.await;

		Ok(EthereumTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.first())?)
				.try_into()
				.expect("Base fee should fit u128"),
			priority_fee: (*context!(context!(fee_history.reward.first())?.first())?)
				.try_into()
				.expect("Priority fee should fit u128"),
		})
	}
}

pub async fn test<StateChainClient, StateChainStream>(
	settings: Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let _ = task_scope::task_scope(|scope| {
		async {
			let eth_client = EthersRetryRpcClient::new(
				scope,
				EthersRpcClient::new(&settings.eth).await.unwrap(),
				settings.eth.ws_node_endpoint,
				web3::types::U256::from(1337),
			);

			let eth_source = EthSource::new(eth_client.clone());

			let epoch_source =
				EpochSource::new(scope, state_chain_stream, state_chain_client.clone())
					.await
					.participating(state_chain_client.account_id())
					.await;

			eth_source
				.shared(scope)
				.chunk_by_time(epoch_source)
				.await
				.chain_tracking(state_chain_client, eth_client)
				.run()
				.await;

			Ok(())
		}
		.boxed()
	})
	.await;
}
