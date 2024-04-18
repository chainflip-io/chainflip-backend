use ethers::prelude::*;

use crate::evm::rpc::{
	address_checker::{AddressCheckerRpcApi, *},
	EvmRpcApi,
};

use super::EvmRetryRpcClient;

use crate::evm::retry_rpc::RequestLog;

#[async_trait::async_trait]
pub trait AddressCheckerRetryRpcApi {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<AddressState>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<U256>;
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + AddressCheckerRpcApi> AddressCheckerRetryRpcApi for EvmRetryRpcClient<Rpc> {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<AddressState> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					let addresses = addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.address_states(block_hash, contract_address, addresses).await
					})
				}),
				RequestLog::new(
					"address_states".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
			)
			.await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<U256> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					let addresses = addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.balances(block_hash, contract_address, addresses).await
					})
				}),
				RequestLog::new(
					"balances".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
			)
			.await
	}
}
