use std::sync::Arc;

use bitcoin::BlockHash;
use utilities::task_scope::task_scope;

use crate::{
	btc::{
		retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
		rpc::BtcRpcClient,
	},
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};
use cf_chains::btc::{BitcoinFeeInfo, BitcoinTrackedData};
use futures::FutureExt;

use super::{
	chain_source::{btc_source::BtcSource, extension::ChainSourceExt, Header},
	chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
	epoch_source::EpochSource,
};

#[async_trait::async_trait]
impl<T: BtcRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Bitcoin, BlockHash, ()>
	for T
{
	async fn get_tracked_data(
		&self,
		_header: &Header<<cf_chains::Bitcoin as cf_chains::Chain>::ChainBlockNumber, BlockHash, ()>,
	) -> Result<<cf_chains::Bitcoin as cf_chains::Chain>::TrackedData, anyhow::Error> {
		// TODO: Bitcoin should return something every block. PRO-481
		if let Some(next_block_fee_rate) = self.next_block_fee_rate().await {
			Ok(BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(next_block_fee_rate) })
		} else {
			Err(anyhow::anyhow!("No fee rate returned"))
		}
	}
}

pub async fn bitcoin_chain_tracking<StateChainClient, StateChainStream>(
	settings: Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	task_scope(|scope| {
		async {
			let btc_client =
				BtcRetryRpcClient::new(scope, BtcRpcClient::new(&settings.btc).unwrap());

			let epoch_source =
				EpochSource::new(scope, state_chain_stream, state_chain_client.clone())
					.await
					.participating(state_chain_client.account_id())
					.await;

			BtcSource::new(btc_client.clone())
				.shared(scope)
				.chunk_by_time(epoch_source)
				.await
				.chain_tracking(state_chain_client, btc_client)
				.run()
				.await;

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap()
}
