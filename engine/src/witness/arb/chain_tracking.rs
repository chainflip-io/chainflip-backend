use crate::{
	evm::retry_rpc::{node_interface::NodeInterfaceRetryRpcApi, EvmRetryRpcApi},
	witness::common::chain_source::Header,
};

use cf_chains::arb::ArbitrumTrackedData;
use ethers::types::Bloom;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::{Bytes, H256};
use frame_support::sp_runtime::FixedU64;
use sp_core::H160;

// TODO: We should get this from a live network and match it with some real reference gas prices for
// ingress/egress gas costs and any other hardcoded gas amounts.
const REFERENCE_GAS_LIMIT: u64 = 1_857_856;

#[async_trait::async_trait]
impl<T: EvmRetryRpcApi + NodeInterfaceRetryRpcApi + Send + Sync + Clone>
	GetTrackedData<cf_chains::Arbitrum, H256, Bloom> for T
{
	async fn get_tracked_data(
		&self,
		_header: &Header<<cf_chains::Arbitrum as cf_chains::Chain>::ChainBlockNumber, H256, Bloom>,
	) -> Result<<cf_chains::Arbitrum as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let gas_estimate_components = self
			.gas_estimate_components(
				// Using zero address as a proxy destination address for the gas estimation.
				H160::default(),
				false,
				// Using empty data for the gas estimation
				Bytes::default(),
			)
			.await;

		let (gas_estimated, _, l2_base_fee, _) = gas_estimate_components;

		Ok(ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().expect("Base fee should fit u128"),
			gas_limit_multiplier: FixedU64::from_rational(
				gas_estimated as u128,
				REFERENCE_GAS_LIMIT as u128,
			),
		})
	}
}
