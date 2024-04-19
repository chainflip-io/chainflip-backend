use std::str::FromStr;

use crate::{evm::retry_rpc::EvmRetryRpcApi, witness::common::chain_source::Header};
// use crate::evm::rpc::node_interface::NodeInterfaceRpcApi;
use crate::evm::retry_rpc::node_interface::NodeInterfaceRetryRpcApi;

use cf_chains::arb::ArbitrumTrackedData;
use ethers::types::Bloom;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::{Bytes, H256};
use frame_support::sp_runtime::FixedU64;
use sp_core::H160;

// TODO: Double check that this this can be any arbitrary address
const DESTINATION_ADDRESS: H160 = H160([
	0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
	0x12, 0x23, 0x34, 0x45,
]);
// TODO: This should be at least the length of the CCM execute call and we should also check how it
// increases with lenght. Double check that this this can be any arbitrary address.
const TX_DATA: &str = "1234567890abcdef1234"; // 10 bytes (hex)
const BYTES_LENGHT_DIV_10: usize = 10; // 10*10 = 100 bytes

// TODO: We should get this from a live network and match it with some real reference gas prices for
// ingress/egress gas costs and any other hardcoded gas amounts.
const REFERENCE_GAS_LIMIT: u64 = 1_291_895;

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
				DESTINATION_ADDRESS,
				false,
				Bytes::from(hex::decode(TX_DATA.repeat(BYTES_LENGHT_DIV_10)).unwrap()),
			)
			.await;

		let (gas_estimated, _, l2_base_fee, _) = gas_estimate_components;

		let gas_limit_multiplier: FixedU64 =
			FixedU64::from_rational(gas_estimated as u128, REFERENCE_GAS_LIMIT as u128);

		Ok(ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().expect("Base fee should fit u128"),
			gas_limit_multiplier,
		})
	}
}
