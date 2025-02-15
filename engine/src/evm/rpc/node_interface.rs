use ethers::prelude::*;
use sp_core::H160;
use std::str::FromStr;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(NodeInterface, "$CF_ARB_CONTRACT_ABI_ROOT/INodeInterface.json");

// It's not actually deployed on-chain but it's accessible via RPC's. See:
// https://docs.arbitrum.io/build-decentralized-apps/nodeinterface/reference
const NODE_INTERFACE_ADDRESS: &str = "0x00000000000000000000000000000000000000C8";

#[async_trait::async_trait]
pub trait NodeInterfaceRpcApi {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)>;
}

#[async_trait::async_trait]
impl NodeInterfaceRpcApi for EvmRpcClient {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)> {
		Ok(NodeInterface::new(
			H160::from_str(NODE_INTERFACE_ADDRESS).unwrap(),
			self.provider.clone(),
		)
		.gas_estimate_components(destination_address, contract_creation, tx_data)
		.call()
		.await?)
	}
}

#[async_trait::async_trait]
impl NodeInterfaceRpcApi for EvmRpcSigningClient {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> Result<(u64, u64, U256, U256)> {
		self.rpc_client
			.gas_estimate_components(destination_address, contract_creation, tx_data)
			.await
	}
}

#[cfg(test)]
mod tests {

	use crate::evm::rpc::EvmRpcApi;
	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires connection to mainnet"]
	async fn arb_node_interface_test() {
		let settings = Settings::new_test().unwrap();

		let client = EvmRpcSigningClient::new(
			settings.eth.private_key_file,
			"https://localhost:9944".into(),
			42161u64,
			"Arbitrum",
		)
		.unwrap()
		.await;
		let chain_id = client.chain_id().await.unwrap();
		println!("chain_id: {:?}", chain_id);

		let (_, _, l2_base_fee, l1_base_fee_estimate) = client
			.gas_estimate_components(
				H160::default(),
				false,
				Bytes::default(),
			)
			.await.unwrap();
		println!("l2_base_fee: {:?}", l2_base_fee);
		println!("l1_base_fee_estimate: {:?}", l1_base_fee_estimate);

		let arb_tracked_data = cf_chains::arb::ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().unwrap(),
			l1_base_fee_estimate: l1_base_fee_estimate.try_into().unwrap(),
		};

		const GAS_BUDGET: u128 = 300_000u128;
		const MESSAGE_LENGTH: usize = 5000;

		let gas_limit_message = arb_tracked_data.calculate_ccm_gas_limit(true, GAS_BUDGET, MESSAGE_LENGTH);
		println!("Message length: {} Gas limit: {}", MESSAGE_LENGTH, gas_limit_message);
		let gas_limit_no_message = arb_tracked_data.calculate_ccm_gas_limit(true, GAS_BUDGET, 0);
		println!("Message length: 0 Gas limit: {}", gas_limit_no_message);
		println!("Difference: {}, Gas Budget {}", gas_limit_message - gas_limit_no_message, GAS_BUDGET);

	}
}
