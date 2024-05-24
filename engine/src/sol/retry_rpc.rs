use crate::{
	retrier::{Attempt, RequestLog, RetrierClient},
	settings::{NodeContainer, WsHttpEndpoints},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::{
	sol::{sol_tx_core::Pubkey, SolHash},
	Solana,
};
use core::time::Duration;
use utilities::task_scope::Scope;

use anyhow::Result;
use std::str::FromStr;

use super::{
	commitment_config::CommitmentConfig,
	rpc::{SolRpcApi, SolRpcClient},
	rpc_client_api::*,
};

#[derive(Clone)]
pub struct SolRetryRpcClient {
	rpc_retry_client: RetrierClient<SolRpcClient>,
	witness_period: u64,
}

const SOLANA_RPC_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

#[allow(dead_code)]
const MAX_BROADCAST_RETRIES: Attempt = 10;

impl SolRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_genesis_hash: Option<SolHash>,
		witness_period: u64,
	) -> Result<Self> {
		// Passing only the http_endpoint. Not using the ws for now
		let rpc_client = SolRpcClient::new(nodes.primary.http_endpoint, expected_genesis_hash)?;

		let backup_rpc_client = nodes
			.backup
			.map(|backup_endpoint| {
				SolRpcClient::new(backup_endpoint.http_endpoint, expected_genesis_hash)
			})
			.transpose()?;

		Ok(Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				"sol_rpc",
				rpc_client,
				backup_rpc_client,
				SOLANA_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			witness_period,
		})
	}
}

#[async_trait::async_trait]
pub trait SolRetryRpcApi: Clone {
	async fn get_block(&self, slot: u64, config: RpcBlockConfig) -> UiConfirmedBlock;
	async fn get_slot(&self, commitment: CommitmentConfig) -> u64; // Slot
	async fn get_recent_prioritization_fees(&self) -> Vec<RpcPrioritizationFee>;
	async fn get_multiple_accounts_with_config(
		&self,
		pubkeys: &[Pubkey],
		config: RpcAccountInfoConfig,
	) -> Response<Vec<Option<UiAccount>>>;
}

#[async_trait::async_trait]
impl SolRetryRpcApi for SolRetryRpcClient {
	async fn get_block(&self, slot: u64, config: RpcBlockConfig) -> UiConfirmedBlock {
		self.rpc_retry_client
			.request(
				RequestLog::new("getBlock".to_string(), Some(format!("{slot:?}, {config:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_block(slot, config).await })
				}),
			)
			.await
	}

	async fn get_slot(&self, commitment: CommitmentConfig) -> u64 {
		self.rpc_retry_client
			.request(
				RequestLog::new("getSlot".to_string(), Some(format!("{commitment:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_slot(commitment).await })
				}),
			)
			.await
	}

	async fn get_recent_prioritization_fees(&self) -> Vec<RpcPrioritizationFee> {
		self.rpc_retry_client
			.request(
				RequestLog::new("getRecentPrioritizationFees".to_string(), None),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_recent_prioritization_fees().await })
				}),
			)
			.await
	}

	async fn get_multiple_accounts_with_config(
		&self,
		pubkeys: &[Pubkey],
		config: RpcAccountInfoConfig,
	) -> Response<Vec<Option<UiAccount>>> {
		let pubkeys = pubkeys.to_owned();
		let config = config.clone();
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getMultipleAccounts".to_string(),
					Some(format!("{:?}, {:?}", pubkeys, config)),
				),
				Box::pin(move |client| {
					let pubkeys = pubkeys.clone();
					let config = config.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.get_multiple_accounts_with_config(&pubkeys, config).await
					})
				}),
			)
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for SolRetryRpcClient {
	type Index = <Solana as cf_chains::Chain>::ChainBlockNumber;
	type Hash = SolHash;
	type Data = ();

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

						async fn get_block_details<Rpc: SolRpcApi>(
							client: &Rpc,
							index: u64,
						) -> anyhow::Result<(SolHash, Option<SolHash>)> {
							let block = client
								.get_block(
									index,
									RpcBlockConfig {
										encoding: Some(UiTransactionEncoding::JsonParsed),
										transaction_details: Some(TransactionDetails::None),
										rewards: Some(false),
										commitment: Some(CommitmentConfig::finalized()),
										max_supported_transaction_version: None,
									},
								)
								.await?;

							let block_hash = block.blockhash;
							Ok((
								SolHash::from_str(&block_hash).expect("Invalid block hash"),
								if index == 0 {
									None
								} else {
									Some(
										SolHash::from_str(&block.previous_blockhash)
											.expect("Invalid parent block hash"),
									)
								},
							))
						}

						let (block_hash, block_parent_hash) =
							get_block_details(&client, *witness_range.end()).await?;

						Ok(Header {
							index: witness_period::block_witness_root(witness_period, index),
							hash: block_hash,
							parent_hash: {
								if witness_range.end() == witness_range.start() {
									block_parent_hash
								} else {
									let (_, parent_block_hash) =
										get_block_details(&client, *witness_range.start()).await?;
									parent_block_hash
								}
							},
							data: (),
						})
					})
				}),
			)
			.await
	}
}

#[cfg(test)]
mod tests {
	// use crate::settings::Settings;
	use cf_chains::Chain;
	use futures::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	async fn test_sol_retry_rpc() {
		task_scope(|scope| {
			async move {
				// let settings = Settings::new_test().unwrap();

				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "wss://api.testnet.solana.com".into(),
							http_endpoint: "https://api.testnet.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let slot = retry_client.get_slot(CommitmentConfig::finalized()).await;
				println!("slot: {:?}", slot);

				let priority_fees = retry_client.get_recent_prioritization_fees().await;
				println!("priority_fees: {:?}", priority_fees[0]);

				// TODO: To add remaining calls

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
