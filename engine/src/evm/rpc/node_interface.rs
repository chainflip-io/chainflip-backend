use ethers::prelude::*;
use sp_core::H160;
use std::str::FromStr;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(NodeInterface, "$CF_ARB_CONTRACT_ABI_ROOT/INodeInterface.json");

// This is a kind of precompile on Arbitrum (although not deployed on chain)
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
