// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	evm::rpc::EvmRpcApi,
	retrier::{Attempt, RequestLog, RetrierClient, MAX_RPC_RETRY_DELAY},
	settings::{NodeContainer, TronEndpoints},
	tron::{
		cached_rpc::{TronRetryRpcApiWithResult, MAX_RETRY_FOR_WITH_RESULT},
		rpc::TronSigningRpcApi,
		rpc_client_api::TransactionResultStatus,
	},
};
use cf_utilities::task_scope::Scope;
use core::time::Duration;
use ethers::types::{Block, Filter, Log, TransactionReceipt, H160, H256, U256, U64};
use futures::future;

use anyhow::{anyhow, Context, Result};

use super::{
	rpc::{TronRpcApi, TronRpcClient, TronRpcSigningClient},
	rpc_client_api::{
		BlockBalance, BlockNumber, Transaction, TransactionInfo, TriggerConstantContractRequest,
		TriggerSmartContractRequest, TronAddress,
	},
};

#[derive(Clone)]
pub struct TronRetryRpcClient<Rpc: TronRpcApi> {
	rpc_retry_client: RetrierClient<Rpc>,
	chain_name: &'static str,
}

const TRON_RPC_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;
const MAX_BROADCAST_RETRIES: Attempt = 2;

impl<Rpc: TronRpcApi> TronRetryRpcClient<Rpc> {
	fn from_inner_clients(
		scope: &Scope<'_, anyhow::Error>,
		rpc_client: Rpc,
		backup_rpc_client: Option<Rpc>,
		tron_rpc_client_name: &'static str,
		chain_name: &'static str,
	) -> Self {
		let rpc_retry_client = RetrierClient::new(
			scope,
			tron_rpc_client_name,
			future::ready(rpc_client),
			backup_rpc_client.map(future::ready),
			TRON_RPC_TIMEOUT,
			MAX_RPC_RETRY_DELAY,
			MAX_CONCURRENT_SUBMISSIONS,
		);
		Self { rpc_retry_client, chain_name }
	}
}

impl TronRetryRpcClient<TronRpcClient> {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<TronEndpoints>,
		expected_chain_id: U256,
		tron_rpc_client_name: &'static str,
		chain_name: &'static str,
	) -> Result<Self> {
		let rpc_client_fut = TronRpcClient::new(
			nodes.primary.http_endpoint.clone(),
			nodes.primary.json_rpc_endpoint.clone(),
			expected_chain_id.as_u64(),
			chain_name,
		)?;
		let rpc_client = rpc_client_fut.await?;

		let backup_rpc_client = match &nodes.backup {
			Some(ep) => {
				let fut = TronRpcClient::new(
					ep.http_endpoint.clone(),
					ep.json_rpc_endpoint.clone(),
					expected_chain_id.as_u64(),
					chain_name,
				)?;
				Some(fut.await?)
			},
			None => None,
		};

		Ok(Self::from_inner_clients(
			scope,
			rpc_client,
			backup_rpc_client,
			tron_rpc_client_name,
			chain_name,
		))
	}
}

impl TronRetryRpcClient<TronRpcSigningClient> {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<TronEndpoints>,
		expected_chain_id: U256,
		tron_rpc_client_name: &'static str,
		chain_name: &'static str,
		private_key_file: std::path::PathBuf,
	) -> Result<Self> {
		let rpc_client_fut = TronRpcSigningClient::new(
			private_key_file.clone(),
			nodes.primary.http_endpoint.clone(),
			nodes.primary.json_rpc_endpoint.clone(),
			expected_chain_id.as_u64(),
			chain_name,
		)?;
		let rpc_client = rpc_client_fut.await?;

		let backup_rpc_client = match &nodes.backup {
			Some(ep) => {
				let fut = TronRpcSigningClient::new(
					private_key_file.clone(),
					ep.http_endpoint.clone(),
					ep.json_rpc_endpoint.clone(),
					expected_chain_id.as_u64(),
					chain_name,
				)?;
				Some(fut.await?)
			},
			None => None,
		};

		Ok(Self::from_inner_clients(
			scope,
			rpc_client,
			backup_rpc_client,
			tron_rpc_client_name,
			chain_name,
		))
	}
}

#[async_trait::async_trait]
pub trait TronRetryRpcApi: Clone {
	// Tron HTTP API methods
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> TransactionInfo;
	async fn get_transaction_by_id(&self, tx_id: &str) -> Transaction;
	async fn get_block_balances(&self, block_number: BlockNumber, hash: &str) -> BlockBalance;

	// EVM-compatible JSON-RPC methods (via Tron's JSON-RPC)
	async fn chain_id(&self) -> U256;
	async fn get_logs(&self, filter: Filter) -> Vec<Log>;
	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt;
	async fn block(&self, block_number: U64) -> Block<H256>;
	async fn block_by_hash(&self, block_hash: H256) -> Block<H256>;
	async fn block_with_txs(&self, block_number: U64) -> Block<ethers::types::Transaction>;
	async fn get_transaction(&self, tx_hash: H256) -> ethers::types::Transaction;
	async fn get_block_number(&self) -> U64;
}

#[async_trait::async_trait]
pub trait TronRetrySigningRpcApi {
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::tron::TronTransaction,
	) -> anyhow::Result<H256>;
}

#[async_trait::async_trait]
impl<Rpc: TronRpcApi + EvmRpcApi> TronRetryRpcApi for TronRetryRpcClient<Rpc> {
	// Tron HTTP API methods
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> TransactionInfo {
		let tx_id = tx_id.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new("getTransactionInfoById".to_string(), Some(format!("{:?}", tx_id))),
				Box::pin(move |client| {
					let tx_id = tx_id.clone();
					Box::pin(async move { client.get_transaction_info_by_id(&tx_id).await })
				}),
			)
			.await
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> Transaction {
		let tx_id = tx_id.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new("getTransactionById".to_string(), Some(format!("{:?}", tx_id))),
				Box::pin(move |client| {
					let tx_id = tx_id.clone();
					Box::pin(async move { client.get_transaction_by_id(&tx_id).await })
				}),
			)
			.await
	}

	async fn get_block_balances(&self, block_number: BlockNumber, hash: &str) -> BlockBalance {
		let hash = hash.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getBlockBalance".to_string(),
					Some(format!("num: {:?}, hash: {}", block_number, hash)),
				),
				Box::pin(move |client| {
					let hash = hash.clone();
					Box::pin(async move { client.get_block_balances(block_number, &hash).await })
				}),
			)
			.await
	}

	// EVM-compatible JSON-RPC methods
	async fn chain_id(&self) -> U256 {
		self.rpc_retry_client
			.request(
				RequestLog::new("eth_chainId".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.chain_id().await })),
			)
			.await
	}

	async fn get_logs(&self, filter: Filter) -> Vec<Log> {
		self.rpc_retry_client
			.request(
				RequestLog::new("eth_getLogs".to_string(), Some(format!("{:?}", filter))),
				Box::pin(move |client| {
					let filter = filter.clone();
					Box::pin(async move { client.get_logs(filter).await })
				}),
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getTransactionReceipt".to_string(),
					Some(format!("{:?}", tx_hash)),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
			)
			.await
	}

	async fn block(&self, block_number: U64) -> Block<H256> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getBlockByNumber".to_string(),
					Some(format!("{:?}", block_number)),
				),
				Box::pin(move |client| Box::pin(async move { client.block(block_number).await })),
			)
			.await
	}

	async fn block_by_hash(&self, block_hash: H256) -> Block<H256> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getBlockByHash".to_string(),
					Some(format!("{:?}", block_hash)),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.block_by_hash(block_hash).await })
				}),
			)
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> Block<ethers::types::Transaction> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getBlockWithTxs".to_string(),
					Some(format!("{:?} (with txs)", block_number)),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> ethers::types::Transaction {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getTransactionByHash".to_string(),
					Some(format!("{:?}", tx_hash)),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.get_transaction(tx_hash).await })
				}),
			)
			.await
	}

	async fn get_block_number(&self) -> U64 {
		self.rpc_retry_client
			.request(
				RequestLog::new("eth_blockNumber".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.get_block_number().await })),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: TronRpcApi + EvmRpcApi> TronRetryRpcApiWithResult for TronRetryRpcClient<Rpc> {
	// Tron HTTP API
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		let tx_id = tx_id.to_owned();
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("getTransactionInfoById".to_string(), Some(tx_id.clone())),
				Box::pin(move |client| {
					let tx_id = tx_id.clone();
					Box::pin(async move { client.get_transaction_info_by_id(&tx_id).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<Transaction> {
		let tx_id = tx_id.to_owned();
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("getTransactionById".to_string(), Some(tx_id.clone())),
				Box::pin(move |client| {
					let tx_id = tx_id.clone();
					Box::pin(async move { client.get_transaction_by_id(&tx_id).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		let hash = hash.to_owned();
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"getBlockBalance".to_string(),
					Some(format!("num: {block_number}, hash: {hash}")),
				),
				Box::pin(move |client| {
					let hash = hash.clone();
					Box::pin(async move { client.get_block_balances(block_number, &hash).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	// EVM-compatible JSON-RPC
	async fn chain_id(&self) -> anyhow::Result<U256> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("eth_chainId".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.chain_id().await })),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> anyhow::Result<Vec<Log>> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"eth_getLogs".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
				Box::pin(move |client| {
					Box::pin(async move {
						client
							.get_logs(
								Filter::new().address(contract_address).at_block_hash(block_hash),
							)
							.await
					})
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"eth_getTransactionReceipt".to_string(),
					Some(format!("{tx_hash:?}")),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"eth_getBlockByNumber".to_string(),
					Some(format!("{block_number}")),
				),
				Box::pin(move |client| Box::pin(async move { client.block(block_number).await })),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("eth_getBlockByHash".to_string(), Some(format!("{block_hash:?}"))),
				Box::pin(move |client| {
					Box::pin(async move { client.block_by_hash(block_hash).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn block_with_txs(
		&self,
		block_number: U64,
	) -> anyhow::Result<Block<ethers::types::Transaction>> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"eth_getBlockWithTxs".to_string(),
					Some(format!("{block_number} (with txs)")),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<ethers::types::Transaction> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"eth_getTransactionByHash".to_string(),
					Some(format!("{tx_hash:?}")),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.get_transaction(tx_hash).await })
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}

	async fn get_block_number(&self) -> anyhow::Result<U64> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new("eth_blockNumber".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.get_block_number().await })),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: TronSigningRpcApi> TronRetrySigningRpcApi for TronRetryRpcClient<Rpc> {
	async fn broadcast_transaction(
		&self,
		transaction: cf_chains::tron::TronTransaction,
	) -> anyhow::Result<H256> {
		let _s = self.chain_name.to_owned();
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"broadcastTransaction".to_string(),
					Some(format!("{:?}", transaction)),
				),
				Box::pin(move |client| {
					let transaction = transaction.clone();
					let signer_address = TronAddress::from_evm_address(client.address());
					let contract_address = TronAddress::from_evm_address(transaction.contract);
					Box::pin(async move {
						let function_selector = std::str::from_utf8(&transaction.function_selector)?
							.to_string();
						let parameter = {
							// We could consider taking the first four bytes (the function
							// selector) and compare them against the hashed
							// function signature but it shouldn't be needed.
							if transaction.data.len() < 4 {
								return Err(anyhow::anyhow!(
									"transaction data must be at least 4 bytes"
								));
							}
							transaction.data[4..].to_vec()
						};


						let constant_request = TriggerConstantContractRequest {
							owner_address: signer_address.clone(),
							contract_address: contract_address.clone(),
							function_selector: function_selector.clone(),
							parameter: parameter.clone(),
						};


						// We estimate the energy needed for the transaction to determine the fee limit. If the transaction
						// reverts due to a logic error, this estimation should fail with a code and message. Then we
						// determine the fee limit depending on whether transaction has a set fee limit (CCM) or
						// if it has one and then we apply a multiplier for safety.
						let energy_estimate = client
							.estimate_energy(constant_request.clone())
							.await
							.context("Failed to estimate energy rpc call failed")?;

						let fee_limit: i64 = match (energy_estimate.result.result, energy_estimate.energy_required) {
							(Some(true), Some(estimated_energy)) => {
								if estimated_energy <= 0 {
									return Err(anyhow::anyhow!("estimated_energy must be greater than 0, got {}", estimated_energy));
								}

								let estimated_fee_limit_sun = (estimated_energy as u128)
									.saturating_mul(cf_chains::tron::fees::SUN_PER_TRX)
									.saturating_div(cf_chains::tron::fees::ENERGY_PER_TX_TRX_BURN);

								if let Some(fee_limit) = transaction.fee_limit {
									let fee_limit_u128: u128 = fee_limit.into();
									if estimated_fee_limit_sun > fee_limit_u128 {
										return Err(anyhow::anyhow!(
											"Estimated fee limit: {} SUN, is greater than the transaction fee limit {} SUN", estimated_fee_limit_sun, fee_limit_u128
										));
									}
									estimated_fee_limit_sun
								} else {
									// Apply a 33% buffer on top of the estimated fee limit for non-CCM transactions
									estimated_fee_limit_sun
										.saturating_mul(133)
										.saturating_div(100)
								}
							},
							_ => {
								let mut details = Vec::new();
								if let Some(code) = &energy_estimate.result.code {
									details.push(format!("code: {code}"));
								}
								if let Some(message) = &energy_estimate.result.message {
									details.push(format!("message: {message}"));
								}
								let suffix = if details.is_empty() {
									String::new()
								} else {
									format!(" ({})", details.join(", "))
								};
								return Err(anyhow::anyhow!(
									"Failed to estimate energy{suffix}"
								));
							},
						}.min(i64::MAX as u128) as i64;

						// Iff the estimate energy is reliable in both revertions AND energy exceed scenarios,
						// then we can skip this step and just rely on the estimate energy. It should be the
						// case but for now we have this just in case.
						let transaction_simulation_result =
							client
								.trigger_constant_contract(constant_request)
								.await
								.context("Failed to simulate the transaction")?.transaction;

						match transaction_simulation_result.status() {
							TransactionResultStatus::Failure => {
								return Err(anyhow::anyhow!(
									"Simulation of the transaction failed, not broadcasting"
								));
							},
							TransactionResultStatus::Unknown => {
								return Err(anyhow::anyhow!(
									"Simulation of the transaction returned unknown status, not broadcasting"
								));
							},
							TransactionResultStatus::Success => {},
						}

						// Then build the actual transaction with triggerSmartContract (includes fee_limit).
						// We need this because the raw_hex_data from the triggerConstantContract does not
						// contain all the data for the valid transaction (e.g. energy limit).
						let trigger_request = TriggerSmartContractRequest {
							owner_address: signer_address.clone(),
							contract_address: contract_address.clone(),
							function_selector: function_selector.clone(),
							parameter: parameter.clone(),
							fee_limit,
						};

						let transaction =
							client
								.trigger_contract(trigger_request)
								.await
								.context("Failed to build the unsigned transaction")?.transaction;

						match transaction.status() {
							TransactionResultStatus::Failure => {
								return Err(anyhow::anyhow!(
									"Transaction result failed, not broadcasting"
								));
							},
							// For non-simulation, unknown is also valid
							TransactionResultStatus::Success | TransactionResultStatus::Unknown => {},
						}

						let returned_fee_limit = transaction.raw_data.fee_limit
							.ok_or_else(|| anyhow!("Transaction raw_data is missing fee_limit"))?;
						if returned_fee_limit != fee_limit {
							return Err(anyhow!(
								"fee_limit mismatch: expected {}, got {}",
								fee_limit,
								returned_fee_limit
							));
						}

						// Decode the raw_data_hex to bytes and sign
						let raw_data_bytes = hex::decode(&transaction.raw_data_hex)
							.map_err(|e| anyhow!("Failed to decode raw_data_hex: {}", e))?;
						let signature = client.sign_raw_bytes(raw_data_bytes).context("Failed to sign the transaction")?;

						// Broadcast the signed transaction
						let raw_data_json = serde_json::to_value(&transaction.raw_data)
							.context("Failed to serialize raw_data")?;
						let response = client
							.broadcast_transaction(
								transaction.tx_id,
								raw_data_json,
								transaction.raw_data_hex,
								vec![signature],
							)
							.await
							.context("Failed to broadcast the transaction")?;

						// Check if the broadcast was successful
						if !response.result {
							let error_message =
								response.message.as_deref().unwrap_or("Unknown error");
							let error_code = response
								.code
								.as_deref()
								.map(|c| format!(" (code: {})", c))
								.unwrap_or_default();
							return Err(anyhow!(
								"Transaction broadcast failed: {}{}",
								error_message,
								error_code
							));
						}

						// The transaction ID is already in the transaction
						Ok(transaction.tx_id)
					})
				}),
				MAX_BROADCAST_RETRIES,
			)
			.await
	}
}

#[cfg(test)]
pub mod mocks {
	use super::*;
	use mockall::mock;

	mock! {
		pub TronRetryRpcClient {}

		impl Clone for TronRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl TronRetrySigningRpcApi for TronRetryRpcClient {
			async fn broadcast_transaction(&self, tx: cf_chains::tron::TronTransaction) -> anyhow::Result<H256>;
		}
	}
}

#[cfg(test)]
mod tests {
	use cf_utilities::{redact_endpoint_secret::SecretUrl, task_scope::task_scope};
	use futures::FutureExt;
	use std::{path::PathBuf, str::FromStr};

	use super::*;

	#[tokio::test]
	#[ignore = "requires access to external RPC and private key"]
	async fn test_tron_send_transaction() {
		task_scope(|scope| {
			async move {
				// Fill in the path to your private key file
				let private_key_file =
					PathBuf::from("/home/albert/work/backend_tron/chainflip-backend/tron_private_key");

				// Tron Mainnet endpoints
				let tron_signing_client = TronRetryRpcClient::<TronRpcSigningClient>::new(
					scope,
					crate::settings::NodeContainer {
						primary: TronEndpoints {
							http_endpoint: SecretUrl::from(
								"https://nile.trongrid.io/wallet".to_string(),
							),
							json_rpc_endpoint: SecretUrl::from(
								"https://nile.trongrid.io/jsonrpc".to_string(),
							),
						},
						backup: None,
					},
					ethers::types::U256::from(3448148188u64), //  Nile testnet chain ID (0xcd8690dc)
					"tron_rpc",
					"Tron",
					private_key_file,
				)
				.await
				.unwrap();

				// Create a transaction request to transfer USDT
				let tx_request = cf_chains::tron::TronTransaction {
					contract: sp_core::H160::from_str("0xeca9bc828a3005b9a3b909f2cc5c2a54794de05f").unwrap(),
					function_selector: "transfer(address,uint256)".to_string().into(),
					data: hex::decode(
						"a9059cbb00000000000000000000004115208EF33A926919ED270E2FA61367B2DA3753DA0000000000000000000000000000000000000000000000000000000000000032"
					)
					.unwrap(),
					fee_limit: Some(1000000000),
					value: Default::default(),
				};

				// Send the transaction (simulate/encode + sign + broadcast)
				let tx_hash = tron_signing_client.broadcast_transaction(tx_request).await.unwrap();

				println!("Transaction sent successfully!");
				println!("Transaction hash: {:x}", tx_hash);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
