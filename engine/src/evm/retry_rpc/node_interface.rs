use ethers::prelude::*;

use crate::evm::rpc::{node_interface::NodeInterfaceRpcApi, EvmRpcApi};

use super::EvmRetryRpcClient;

use crate::evm::retry_rpc::RequestLog;

#[async_trait::async_trait]
pub trait NodeInterfaceRetryRpcApi {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> (u64, u64, U256, U256);
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + NodeInterfaceRpcApi> NodeInterfaceRetryRpcApi for EvmRetryRpcClient<Rpc> {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> (u64, u64, U256, U256) {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					let tx_data = tx_data.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client
							.gas_estimate_components(
								destination_address,
								contract_creation,
								tx_data,
							)
							.await
					})
				}),
				RequestLog::new(
					"gas_estimate_components".to_string(),
					Some(format!("{destination_address:?}, {contract_creation:?}")),
				),
			)
			.await
	}
}
