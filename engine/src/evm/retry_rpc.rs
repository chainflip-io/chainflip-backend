pub mod address_checker;
pub mod node_interface;

use ethers::{
	prelude::*,
	types::{transaction::eip2930::AccessList, TransactionReceipt},
};

use futures_core::Future;
use utilities::task_scope::Scope;

use crate::{
	evm::rpc::{EvmRpcApi, EvmSigningRpcApi},
	retrier::{Attempt, RequestLog, RetrierClient},
	settings::{NodeContainer, WsHttpEndpoints},
	witness::common::chain_source::{ChainClient, Header},
};
use std::{path::PathBuf, time::Duration};

use super::{
	rpc::{EvmRpcClient, EvmRpcSigningClient, ReconnectSubscriptionClient},
	ConscientiousEvmWebsocketBlockHeaderStream,
};
use crate::evm::rpc::ReconnectSubscribeApi;
use cf_chains::Ethereum;

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct EvmRetryRpcClient<Rpc: EvmRpcApi> {
	rpc_retry_client: RetrierClient<Rpc>,
	sub_retry_client: RetrierClient<ReconnectSubscriptionClient>,
	chain_name: &'static str,
	witness_period: u64,
}

const ETHERS_RPC_TIMEOUT: Duration = Duration::from_millis(4 * 1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

const MAX_BROADCAST_RETRIES: Attempt = 2;

impl<Rpc: EvmRpcApi> EvmRetryRpcClient<Rpc> {
	fn from_inner_clients<ClientFut: Future<Output = Rpc> + Send + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_chain_id: U256,
		rpc_client: ClientFut,
		backup_rpc_client: Option<ClientFut>,
		evm_rpc_client_name: &'static str,
		evm_subscription_client_name: &'static str,
		chain_name: &'static str,
		witness_period: u64,
	) -> Self {
		let sub_client = ReconnectSubscriptionClient::new(
			nodes.primary.ws_endpoint,
			expected_chain_id,
			chain_name,
		);

		let backup_sub_client = nodes.backup.as_ref().map(|ep| {
			ReconnectSubscriptionClient::new(ep.ws_endpoint.clone(), expected_chain_id, chain_name)
		});

		Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				evm_rpc_client_name,
				rpc_client,
				backup_rpc_client,
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			sub_retry_client: RetrierClient::new(
				scope,
				evm_subscription_client_name,
				futures::future::ready(sub_client),
				backup_sub_client.map(futures::future::ready),
				ETHERS_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			chain_name,
			witness_period,
		}
	}
}

impl EvmRetryRpcClient<EvmRpcClient> {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_chain_id: U256,
		evm_rpc_client_name: &'static str,
		evm_subscription_client_name: &'static str,
		chain_name: &'static str,
		witness_period: u64,
	) -> Result<Self> {
		let rpc_client = EvmRpcClient::new(
			nodes.primary.http_endpoint.clone(),
			expected_chain_id.as_u64(),
			chain_name,
		)?;

		let backup_rpc_client = nodes
			.backup
			.as_ref()
			.map(|ep| {
				EvmRpcClient::new(ep.http_endpoint.clone(), expected_chain_id.as_u64(), chain_name)
			})
			.transpose()?;

		Ok(Self::from_inner_clients(
			scope,
			nodes,
			expected_chain_id,
			rpc_client,
			backup_rpc_client,
			evm_rpc_client_name,
			evm_subscription_client_name,
			chain_name,
			witness_period,
		))
	}
}

impl EvmRetryRpcClient<EvmRpcSigningClient> {
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		private_key_file: PathBuf,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_chain_id: U256,
		evm_rpc_client_name: &'static str,
		evm_subscription_client_name: &'static str,
		chain_name: &'static str,
		witness_period: u64,
	) -> Result<Self> {
		let rpc_client = EvmRpcSigningClient::new(
			private_key_file.clone(),
			nodes.primary.http_endpoint.clone(),
			expected_chain_id.as_u64(),
			chain_name,
		)?;

		let backup_rpc_client = nodes
			.backup
			.as_ref()
			.map(|ep| {
				EvmRpcSigningClient::new(
					private_key_file.clone(),
					ep.http_endpoint.clone(),
					expected_chain_id.as_u64(),
					chain_name,
				)
			})
			.transpose()?;

		Ok(Self::from_inner_clients(
			scope,
			nodes,
			expected_chain_id,
			rpc_client,
			backup_rpc_client,
			evm_rpc_client_name,
			evm_subscription_client_name,
			chain_name,
			witness_period,
		))
	}
}

#[async_trait::async_trait]
pub trait EvmRetryRpcApi: Clone {
	async fn get_logs_range(
		&self,
		range: std::ops::RangeInclusive<u64>,
		contract_address: H160,
	) -> Vec<Log>;

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

	async fn get_transaction(&self, tx_hash: H256) -> Transaction;
}

#[async_trait::async_trait]
pub trait EvmRetrySigningRpcApi: EvmRetryRpcApi {
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::evm::Transaction,
	) -> anyhow::Result<TxHash>;
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi> EvmRetryRpcApi for EvmRetryRpcClient<Rpc> {
	async fn get_logs_range(
		&self,
		range: std::ops::RangeInclusive<u64>,
		contract_address: H160,
	) -> Vec<Log> {
		assert!(!range.is_empty());
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"get_logs_range".to_string(),
					Some(format!("{range:?}, {contract_address:?}")),
				),
				Box::pin(move |client| {
					let range = range.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client
							.get_logs(
								// The `from_block` and `to_block` are inclusive
								Filter::new()
									.address(contract_address)
									.from_block(*range.start())
									.to_block(*range.end()),
							)
							.await
					})
				}),
			)
			.await
	}

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> Vec<Log> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"get_logs".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
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
			)
			.await
	}

	async fn chain_id(&self) -> U256 {
		self.rpc_retry_client
			.request(
				RequestLog::new("chain_id".to_string(), None),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.chain_id().await })
				}),
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt {
		self.rpc_retry_client
			.request(
				RequestLog::new("transaction_receipt".to_string(), Some(format!("{tx_hash:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
			)
			.await
	}

	async fn block(&self, block_number: U64) -> Block<H256> {
		self.rpc_retry_client
			.request(
				RequestLog::new("block".to_string(), Some(format!("{block_number}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block(block_number).await })
				}),
			)
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction> {
		self.rpc_retry_client
			.request(
				RequestLog::new("block_with_txs".to_string(), Some(format!("{block_number}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
			)
			.await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> FeeHistory {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"fee_history".to_string(),
					Some(format!("{block_count}, {newest_block}, {reward_percentiles:?}")),
				),
				Box::pin(move |client| {
					let reward_percentiles = reward_percentiles.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.fee_history(block_count, newest_block, &reward_percentiles).await
					})
				}),
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Transaction {
		self.rpc_retry_client
			.request(
				RequestLog::new("get_transaction".to_string(), Some(format!("{tx_hash:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_transaction(tx_hash).await })
				}),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: EvmSigningRpcApi> EvmRetrySigningRpcApi for EvmRetryRpcClient<Rpc> {
	/// Estimates gas and then sends the transaction to the network.
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::evm::Transaction,
	) -> anyhow::Result<TxHash> {
		let s = self.chain_name.to_owned();
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("broadcast_transaction".to_string(), Some(format!("{tx:?}"))),
				Box::pin(move |client| {
					let tx = tx.clone();
					let s = s.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let mut transaction_request = Eip1559TransactionRequest {
							to: Some(NameOrAddress::Address(tx.contract)),
							data: Some(tx.data.into()),
							chain_id: Some(tx.chain_id.into()),
							value: Some(tx.value),
							max_fee_per_gas: tx.max_fee_per_gas,
							max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
							// geth uses the latest block gas limit as an upper bound
							gas: None,
							access_list: AccessList::default(),
							from: Some(client.address()),
							nonce: None,
						};

						let estimated_gas = client
							.estimate_gas(&transaction_request)
							.await
							.context("Failed to estimate gas")?;

						transaction_request.gas = Some(match tx.gas_limit {
							Some(gas_limit) =>
								if estimated_gas > gas_limit {
									return Err(anyhow::anyhow!(
										"Estimated gas is greater than the gas limit"
									))
								} else {
									gas_limit
								},
							None => {
								// increase the estimate by 33% for normal transactions
								estimated_gas.saturating_mul(U256::from(4u64)) / 3u64
							},
						});

						client
							.send_transaction(transaction_request)
							.await
							.context(format!("Failed to send {} transaction", s))
					})
				}),
				MAX_BROADCAST_RETRIES,
			)
			.await
	}
}

#[async_trait::async_trait]
pub trait EvmRetrySubscribeApi {
	async fn subscribe_blocks(&self) -> ConscientiousEvmWebsocketBlockHeaderStream;
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi> EvmRetrySubscribeApi for EvmRetryRpcClient<Rpc> {
	async fn subscribe_blocks(&self) -> ConscientiousEvmWebsocketBlockHeaderStream {
		self.sub_retry_client
			.request(
				RequestLog::new("subscribe_blocks".to_string(), None),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.subscribe_blocks().await })
				}),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi> ChainClient for EvmRetryRpcClient<Rpc> {
	type Index = <Ethereum as cf_chains::Chain>::ChainBlockNumber;

	type Hash = H256;

	type Data = Bloom;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		use cf_chains::witness_period;

		let witness_period = self.witness_period;
		assert!(witness_period::is_block_witness_root(witness_period, index));
		self.rpc_retry_client
			.request(
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let witness_range =
							witness_period::block_witness_range(witness_period, index);

						async fn get_block_details<Rpc: EvmRpcApi>(
							client: &Rpc,
							index: u64,
						) -> anyhow::Result<(H256, Option<H256>, Bloom)> {
							let block = client.block(index.into()).await?;

							if let (Some(block_number), Some(block_hash)) =
								(block.number, block.hash)
							{
								assert_eq!(block_number.as_u64(), index);
								Ok((
									block_hash,
									if index == 0 { None } else { Some(block.parent_hash) },
									block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)),
								))
							} else {
								Err(anyhow::anyhow!(
									"Block number or hash is none for block number: {}",
									index
								))
							}
						}

						let (block_hash, block_parent_hash, block_bloom) =
							get_block_details(&client, *witness_range.end()).await?;

						Ok(Header {
							index: witness_period::block_witness_root(witness_period, index),
							hash: block_hash,
							parent_hash: {
								if witness_range.end() == witness_range.start() {
									block_parent_hash
								} else {
									let (_, parent_block_hash, _) =
										get_block_details(&client, *witness_range.start()).await?;
									parent_block_hash
								}
							},
							data: block_bloom,
						})
					})
				}),
			)
			.await
	}
}

#[cfg(test)]
pub mod mocks {
	use super::*;
	use mockall::mock;

	mock! {
		pub EvmRetryRpcClient {}

		impl Clone for EvmRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl EvmRetrySigningRpcApi for EvmRetryRpcClient {
			async fn broadcast_transaction(
				&self,
				tx: cf_chains::evm::Transaction,
			) -> anyhow::Result<TxHash>;

		}

		#[async_trait::async_trait]
		impl EvmRetryRpcApi for EvmRetryRpcClient {
			async fn get_logs_range(&self, range: std::ops::RangeInclusive<u64>, contract_address: H160) -> Vec<Log>;

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

			async fn get_transaction(&self, tx_hash: H256) -> Transaction;
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::settings::Settings;
	use cf_chains::Chain;
	use futures::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore = "requires a local node"]
	async fn test_eth_retry_rpc() {
		task_scope(|scope| {
			async move {
				let settings = Settings::new_test().unwrap();

				let retry_client = EvmRetryRpcClient::<EvmRpcSigningClient>::new(
					scope,
					settings.eth.private_key_file,
					settings.eth.nodes,
					U256::from(1337u64),
					"eth_rpc",
					"eth_subscribe",
					"Ethereum",
					Ethereum::WITNESS_PERIOD,
				)
				.unwrap();

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
