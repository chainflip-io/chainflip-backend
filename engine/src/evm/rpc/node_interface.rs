use ethers::prelude::*;
// use ethers::types::U64;
use sp_core::H160;
use std::str::FromStr;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(NodeInterface, "$CF_ARB_CONTRACT_ABI_ROOT/INodeInterface.json");

// This is a kind of precompile on Arbitrum
const NODE_INTERFACE_ADDRESS: &str = "0x00000000000000000000000000000000000000C8";
// TODO: This should be the address of the contract that we are going to call. However, passing the
// vault contract address here is quite annoying and I think it works regardless of the address, so
// I don't think it's necessary. const DESTINATION_ADDRESS: &str =
// "0x0000000000000000000000000000000000000123"; TODO: This should be at least the length of the CCM
// execute call and we should also check how it increases with lenght. const TX_DATA: &str =
// "1234567890abcdef";

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
