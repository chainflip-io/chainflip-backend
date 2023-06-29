use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};

use utilities::task_scope::Scope;

use crate::{eth::ethers_rpc::EthersRpcApi, rpc_retrier::RpcRetrierClient};
use std::{time::Duration};

use super::{
	ethers_rpc::{EthersRpcClient, ReconnectSubscriptionClient},
	ConscientiousEthWebsocketBlockHeaderStream,
};

pub struct EthersRetryRpcClient<T: JsonRpcClient> {
	retry_client: RpcRetrierClient<EthersRpcClient<T>>,
}

const ETHERS_RPC_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

impl<T: JsonRpcClient + Clone + Send + Sync + 'static> EthersRetryRpcClient<T> {
	pub fn new(scope: &Scope<'_, anyhow::Error>, ethers_client: EthersRpcClient<T>) -> Self {
		Self {
			retry_client: RpcRetrierClient::new(
				scope,
				ethers_client,
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		}
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
impl<T: JsonRpcClient + Clone + Send + Sync + 'static> EthersRetryRpcApi
	for EthersRetryRpcClient<T>
{
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

use crate::eth::ethers_rpc::ReconnectSubscribeApi;

pub struct EthersRetrySubscribeRpcClient {
	retry_client: RpcRetrierClient<ReconnectSubscriptionClient>,
}

#[async_trait::async_trait]
pub trait EthersRetrySubscribeApi {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream;
}

#[async_trait::async_trait]
impl EthersRetrySubscribeApi for EthersRetrySubscribeRpcClient {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream {
		self.retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.subscribe_blocks().await })
			}))
			.await
	}
}

#[cfg(test)]
mod tests {
	use crate::settings::Settings;
	use futures::FutureExt;
	use std::sync::Arc;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore = "requires a local node"]
	async fn test_eth_retry_rpc() {
		task_scope(|scope| {
			async move {
				let settings = Settings::new_test().unwrap();
				let client = EthersRpcClient::new(
					Arc::new(Provider::<Http>::try_from(
						settings.eth.http_node_endpoint.to_string(),
					)?),
					&settings.eth,
				)
				.await
				.unwrap();

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
