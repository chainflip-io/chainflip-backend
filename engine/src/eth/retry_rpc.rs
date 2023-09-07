pub mod address_checker;

use ethers::{
	prelude::*,
	types::{
		transaction::{eip2718::TypedTransaction, eip2930::AccessList},
		TransactionReceipt,
	},
};

use futures_core::Future;
use utilities::task_scope::Scope;

use crate::{
	eth::rpc::EthRpcApi,
	retrier::{Attempt, RequestLog, RetrierClient},
	witness::common::chain_source::{ChainClient, Header},
};
use std::time::Duration;

use super::{
	rpc::{EthRpcClient, ReconnectSubscriptionClient},
	ConscientiousEthWebsocketBlockHeaderStream,
};
use crate::eth::rpc::ReconnectSubscribeApi;
use cf_chains::Ethereum;

use anyhow::Context;

#[derive(Clone)]
pub struct EthersRetryRpcClient {
	rpc_retry_client: RetrierClient<EthRpcClient>,
	sub_retry_client: RetrierClient<ReconnectSubscriptionClient>,
}

const ETHERS_RPC_TIMEOUT: Duration = Duration::from_millis(4 * 1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

const MAX_BROADCAST_RETRIES: Attempt = 5;

impl EthersRetryRpcClient {
	pub fn new<EthRpcClientFut: Future<Output = EthRpcClient> + Send + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		rpc_client: EthRpcClientFut,
		backup_rpc_client: Option<EthRpcClientFut>,
		sub_client: ReconnectSubscriptionClient,
		backup_sub_client: Option<ReconnectSubscriptionClient>,
	) -> Self {
		Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				"eth_rpc",
				rpc_client,
				backup_rpc_client,
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			sub_retry_client: RetrierClient::new(
				scope,
				"eth_subscribe",
				futures::future::ready(sub_client),
				backup_sub_client.map(futures::future::ready),
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		}
	}
}

#[async_trait::async_trait]
pub trait EthersRetryRpcApi: Clone {
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::evm::Transaction,
	) -> anyhow::Result<TxHash>;

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> Vec<Log>;

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
	/// Estimates gas and then sends the transaction to the network.
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::evm::Transaction,
	) -> anyhow::Result<TxHash> {
		let log = RequestLog::new("broadcast_transaction".to_string(), Some(format!("{tx:?}")));
		self.rpc_retry_client
			.request_with_limit(
				Box::pin(move |client| {
					let tx = tx.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let mut transaction_request = Eip1559TransactionRequest {
							to: Some(NameOrAddress::Address(tx.contract)),
							data: Some(tx.data.into()),
							chain_id: Some(tx.chain_id.into()),
							value: Some(tx.value),
							max_fee_per_gas: tx.max_fee_per_gas,
							max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
							gas: tx.gas_limit,
							access_list: AccessList::default(),
							from: Some(client.address()),
							nonce: None,
						};

						let estimated_gas = client
							.estimate_gas(&TypedTransaction::Eip1559(transaction_request.clone()))
							.await
							.context("Failed to estimate gas")?;

						// increase the estimate by 50%
						transaction_request.gas = Some(
							estimated_gas
								.saturating_mul(U256::from(3u64))
								.checked_div(U256::from(2u64))
								.unwrap(),
						);

						client
							.send_transaction(transaction_request.into())
							.await
							.context("Failed to send ETH transaction")
					})
				}),
				log,
				MAX_BROADCAST_RETRIES,
			)
			.await
	}

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> Vec<Log> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client
							.get_logs(
								Filter::new().address(contract_address).at_block_hash(block_hash),
							)
							.await
					})
				}),
				RequestLog::new(
					"get_logs".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
			)
			.await
	}

	async fn chain_id(&self) -> U256 {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.chain_id().await })
				}),
				RequestLog::new("chain_id".to_string(), None),
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
				RequestLog::new("transaction_receipt".to_string(), Some(format!("{tx_hash:?}"))),
			)
			.await
	}

	async fn block(&self, block_number: U64) -> Block<H256> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block(block_number).await })
				}),
				RequestLog::new("block".to_string(), Some(format!("{block_number}"))),
			)
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
				RequestLog::new("block_with_txs".to_string(), Some(format!("{block_number}"))),
			)
			.await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> FeeHistory {
		let log = RequestLog::new(
			"fee_history".to_string(),
			Some(format!("{block_count}, {newest_block}, {reward_percentiles:?}")),
		);
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					let reward_percentiles = reward_percentiles.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.fee_history(block_count, newest_block, &reward_percentiles).await
					})
				}),
				log,
			)
			.await
	}
}

#[async_trait::async_trait]
pub trait EthersRetrySubscribeApi {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream;
}

#[async_trait::async_trait]
impl EthersRetrySubscribeApi for EthersRetryRpcClient {
	async fn subscribe_blocks(&self) -> ConscientiousEthWebsocketBlockHeaderStream {
		self.sub_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.subscribe_blocks().await })
				}),
				RequestLog::new("subscribe_blocks".to_string(), None),
			)
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for EthersRetryRpcClient {
	type Index = <Ethereum as cf_chains::Chain>::ChainBlockNumber;

	type Hash = H256;

	type Data = Bloom;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.rpc_retry_client
			.request(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let block = client.block(index.into()).await?;
						let (Some(block_number), Some(block_hash)) = (block.number, block.hash)
						else {
							return Err(anyhow::anyhow!(
								"Block number or hash is none for block number: {}",
								index
							))
						};

						assert_eq!(block_number.as_u64(), index);
						Ok(Header {
							index,
							hash: block_hash,
							parent_hash: Some(block.parent_hash),
							data: block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)).0.into(),
						})
					})
				}),
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
			)
			.await
	}
}

#[cfg(test)]
pub mod mocks {
	use super::*;
	use mockall::mock;

	mock! {
		pub EthRetryRpcClient {}

		impl Clone for EthRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl EthersRetryRpcApi for EthRetryRpcClient {
			async fn broadcast_transaction(
				&self,
				tx: cf_chains::evm::Transaction,
			) -> anyhow::Result<TxHash>;

			async fn get_logs(&self, block_hash: H256, contract_address: H160) -> Vec<Log>;

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

				let eth_rpc_client = EthRpcClient::new(
					settings.eth.private_key_file,
					settings.eth.nodes.primary.http_node_endpoint,
					1337u64,
				)
				.unwrap();
				let retry_client = EthersRetryRpcClient::new(
					scope,
					eth_rpc_client,
					None,
					ReconnectSubscriptionClient::new(
						settings.eth.nodes.primary.ws_node_endpoint,
						web3::types::U256::from(1337),
					),
					None,
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
