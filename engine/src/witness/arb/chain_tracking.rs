use crate::{eth::retry_rpc::EthersRetryRpcApi, witness::common::chain_source::Header};
use cf_chains::arb::ArbitrumTrackedData;
use ethers::types::Bloom;
use sp_core::U256;
use utilities::context;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::H256;

#[async_trait::async_trait]
impl<T: EthersRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Arbitrum, H256, Bloom>
	for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<<cf_chains::Arbitrum as cf_chains::Chain>::ChainBlockNumber, H256, Bloom>,
	) -> Result<<cf_chains::Arbitrum as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let priority_fee_percentile = 50u8;
		let fee_history = self
			.fee_history(
				U256::one(),
				header.index.into(),
				vec![priority_fee_percentile as f64 / 100_f64],
			)
			.await;

		Ok(ArbitrumTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.first())?)
				.try_into()
				.expect("Base fee should fit u128"),
		})
	}
}
