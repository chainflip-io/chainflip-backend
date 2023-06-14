use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};

use utilities::task_scope::Scope;

use crate::{eth::ethers_rpc::EthersRpcApi, rpc_retrier::RpcRetrierClient};
use std::time::Duration;

use super::ethers_rpc::EthersRpcClient;

pub struct EthersRetryRpcClient {
	retry_client: RpcRetrierClient<EthersRpcClient>,
}

const ETHERS_RPC_TIMEOUT: Duration = Duration::from_millis(1000);

impl EthersRetryRpcClient {
	pub fn new(scope: &Scope<'_, anyhow::Error>, ethers_client: EthersRpcClient) -> Self {
		Self { retry_client: RpcRetrierClient::new(scope, ethers_client, ETHERS_RPC_TIMEOUT) }
	}
}

#[async_trait::async_trait]
pub trait EthersRetryRpcApi: Send + Sync {
	async fn estimate_gas(&self, req: TypedTransaction) -> U256;

	async fn send_transaction(&self, tx: TransactionRequest) -> TxHash;

	async fn get_logs(&self, filter: Filter) -> Vec<Log>;

	async fn chain_id(&self) -> U256;

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt;

	async fn block(&self, block_number: U64) -> Block<H256>;

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction>;

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> FeeHistory;
}

#[async_trait::async_trait]
impl EthersRetryRpcApi for EthersRetryRpcClient {
	async fn estimate_gas(&self, req: TypedTransaction) -> U256 {
		self.retry_client
			.request(Box::pin(move |client| {
				let req = req.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.estimate_gas(&req).await })
			}))
			.await
	}

	async fn send_transaction(&self, tx: TransactionRequest) -> TxHash {
		self.retry_client
			.request(Box::pin(move |client| {
				let tx = tx.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.send_transaction(tx).await })
			}))
			.await
	}

	async fn get_logs(&self, filter: Filter) -> Vec<Log> {
		self.retry_client
			.request(Box::pin(move |client| {
				let filter = filter.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.get_logs(filter).await })
			}))
			.await
	}

	async fn chain_id(&self) -> U256 {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.chain_id().await })
			}))
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.transaction_receipt(tx_hash).await })
			}))
			.await
	}

	async fn block(&self, block_number: U64) -> Block<H256> {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block(block_number).await })
			}))
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction> {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block_with_txs(block_number).await })
			}))
			.await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> FeeHistory {
		self.retry_client
			.request(Box::pin(move |client| {
				let reward_percentiles = reward_percentiles.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move {
					client.fee_history(block_count, newest_block, &reward_percentiles).await
				})
			}))
			.await
	}
}

#[cfg(test)]
mod tests {
	use crate::settings::Settings;
	use futures::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore = "requires a local node"]
	async fn test_eth_retry_rpc() {
		task_scope(|scope| {
			async move {
				let settings = Settings::new_test().unwrap();
				let client = EthersRpcClient::new(&settings.eth).await.unwrap();

				let retry_client = EthersRetryRpcClient::new(scope, client);

				let chain_id = retry_client.chain_id().await;
				println!("chain_id: {}", chain_id);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
