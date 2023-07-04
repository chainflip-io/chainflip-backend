use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};

use utilities::task_scope::Scope;

use crate::{
	eth::ethers_rpc::EthersRpcApi,
	rpc_retrier::RpcRetrierClient,
	witness::chain_source::{ChainClient, Header},
};
use std::time::Duration;

use super::{
	ethers_rpc::{EthersRpcClient, ReconnectSubscriptionClient},
	ConscientiousEthWebsocketBlockHeaderStream,
};
use crate::eth::ethers_rpc::ReconnectSubscribeApi;

pub struct EthersRetryRpcClient<T: JsonRpcClient> {
	rpc_retry_client: RpcRetrierClient<EthersRpcClient<T>>,
	sub_retry_client: RpcRetrierClient<ReconnectSubscriptionClient>,
}

const ETHERS_RPC_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

impl<T: JsonRpcClient + Clone + Send + Sync + 'static> EthersRetryRpcClient<T> {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		ethers_client: EthersRpcClient<T>,
		ws_node_endpoint: String,
		chain_id: web3::types::U256,
	) -> Self {
		Self {
			rpc_retry_client: RpcRetrierClient::new(
				scope,
				ethers_client,
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			sub_retry_client: RpcRetrierClient::new(
				scope,
				ReconnectSubscriptionClient::new(ws_node_endpoint, chain_id),
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
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				let req = req.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.estimate_gas(&req).await })
			}))
			.await
	}

	async fn send_transaction(&self, tx: TransactionRequest) -> TxHash {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				let tx = tx.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.send_transaction(tx).await })
			}))
			.await
	}

	async fn get_logs(&self, filter: Filter) -> Vec<Log> {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				let filter = filter.clone();
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.get_logs(filter).await })
			}))
			.await
	}

	async fn chain_id(&self) -> U256 {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.chain_id().await })
			}))
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.transaction_receipt(tx_hash).await })
			}))
			.await
	}

	async fn block(&self, block_number: U64) -> Block<H256> {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.block(block_number).await })
			}))
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction> {
		self.rpc_retry_client
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
		self.rpc_retry_client
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

#[async_trait::async_trait]
pub trait EthersRetrySubscribeApi {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream;
}

#[async_trait::async_trait]
impl<T: JsonRpcClient + Clone> EthersRetrySubscribeApi for EthersRetryRpcClient<T> {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream {
		self.sub_retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move { client.subscribe_blocks().await })
			}))
			.await
	}
}

#[async_trait::async_trait]
impl<T: JsonRpcClient + Clone + Send + Sync + 'static> ChainClient for EthersRetryRpcClient<T> {
	type Index = u64;

	type Hash = H256;

	type Data = ();

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.rpc_retry_client
			.request(Box::pin(move |client| {
				#[allow(clippy::redundant_async_block)]
				Box::pin(async move {
					let block = client.block(index.into()).await?;
					if block.number.is_none() || block.hash.is_none() {
						return Err(anyhow::anyhow!(
							"Block number or hash is none for block number: {}",
							index
						))
					}

					assert_eq!(block.number.unwrap().as_u64(), index);
					Ok(Header {
						index,
						hash: block.hash.unwrap(),
						parent_hash: Some(block.parent_hash),
						data: (),
					})
				})
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

				let retry_client = EthersRetryRpcClient::new(
					scope,
					client,
					settings.eth.ws_node_endpoint,
					web3::types::U256::from(1337),
				);

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
