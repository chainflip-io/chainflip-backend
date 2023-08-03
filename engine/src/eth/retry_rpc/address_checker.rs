use ethers::prelude::*;

use crate::eth::rpc::address_checker::{AddressCheckerRpcApi, *};

use super::EthersRetryRpcClient;

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
impl AddressCheckerRetryRpcApi for EthersRetryRpcClient {
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
				format!("address_states({block_hash}, {contract_address}"),
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
				format!("balances({block_hash}, {contract_address}"),
			)
			.await
	}
}
