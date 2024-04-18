use std::str::FromStr;

use crate::{evm::retry_rpc::EvmRetryRpcApi, witness::common::chain_source::Header};
// use crate::evm::rpc::node_interface::NodeInterfaceRpcApi;
use crate::evm::retry_rpc::node_interface::NodeInterfaceRetryRpcApi;

use cf_chains::arb::ArbitrumTrackedData;
use ethers::types::Bloom;
use sp_core::U256;
use utilities::context;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::{H256, Bytes};
use sp_core::H160;

// TODO: This should be the address of the contract that we are going to call. However, passing the vault contract address
// here is quite annoying and I think it works regardless of the address, so I don't think it's necessary.
const DESTINATION_ADDRESS: &str = "0x0000000000000000000000000000000000000123";
// TODO: This should be at least the length of the CCM execute call and we should also check how it increases with lenght.
const TX_DATA: &str = "1234567890abcdef";


#[async_trait::async_trait]
impl<T: EvmRetryRpcApi + NodeInterfaceRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Arbitrum, H256, Bloom>
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

		let gas_estimate_components = self.gas_estimate_components(
			H160::from_str(DESTINATION_ADDRESS).expect("Destination address should be valid"),
			false,
			Bytes::from(hex::decode(TX_DATA).unwrap()),
		).await;

		let (gas_estimated, _, _, _) = gas_estimate_components;

		Ok(ArbitrumTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.first())?)
				.try_into()
				.expect("Base fee should fit u128"),
			gas_estimate: gas_estimated
			.try_into()
			.expect("Base fee should fit u128"),
		})
	}
}
