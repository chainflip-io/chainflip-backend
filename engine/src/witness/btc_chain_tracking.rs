use bitcoin::BlockHash;

use crate::btc::retry_rpc::BtcRetryRpcApi;
use cf_chains::btc::{BitcoinFeeInfo, BitcoinTrackedData};

use super::{
	chain_source::Header, chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
};

#[async_trait::async_trait]
impl<T: BtcRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Bitcoin, BlockHash, ()>
	for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<<cf_chains::Bitcoin as cf_chains::Chain>::ChainBlockNumber, BlockHash, ()>,
	) -> Result<<cf_chains::Bitcoin as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let fee_rate = if let Some(next_block_fee_rate) = self.next_block_fee_rate().await {
			next_block_fee_rate
		} else {
			self.average_block_fee_rate(header.hash).await
		};

		Ok(BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(fee_rate) })
	}
}
