use crate::eth::retry_rpc::EthersRetryRpcApi;
use cf_chains::{eth::EthereumTrackedData, Ethereum};
use ethers::types::BlockNumber;
use sp_core::U256;

use crate::eth::retry_rpc::EthersRetryRpcClient;

use super::GetChainTrackingData;

const PRIORITY_FEE_PERCENTILE: u8 = 50;

#[async_trait::async_trait]
impl GetChainTrackingData<Ethereum> for EthersRetryRpcClient {
	async fn get_chain_tracking_data(&self) -> EthereumTrackedData {
		let fee_history = self
			.fee_history(
				U256::one(),
				BlockNumber::Latest,
				vec![PRIORITY_FEE_PERCENTILE as f64 / 100_f64],
			)
			.await;

		EthereumTrackedData {
			block_height: fee_history.oldest_block.try_into().unwrap(),
			base_fee: (*fee_history.base_fee_per_gas.first().unwrap())
				.try_into()
				.expect("Base fee should fit u128"),
			priority_fee: (*(*fee_history.reward.first().unwrap()).first().unwrap())
				.try_into()
				.expect("Priority fee should fit u128"),
		}
	}
}
