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
	tron::rpc::TronSigningRpcApi,
};
use cf_utilities::task_scope::Scope;
use core::time::Duration;
use ethers::types::{Block, Filter, Log, Transaction, TransactionReceipt, H256, U256, U64};
use futures::future;

use anyhow::Result;

use super::{
	rpc::{TronRpcApi, TronRpcClient, TronRpcSigningClient},
	rpc_client_api::{
		BlockBalance, BlockNumber, TransactionInfo, TriggerSmartContractRequest, TronAddress,
		TronTransaction, TronTransactionRequest, UnsignedTronTransaction,
	},
};

#[derive(Clone)]
pub struct TronRetryRpcClient<Rpc: TronRpcApi> {
	rpc_retry_client: RetrierClient<Rpc>,
	chain_name: &'static str,
	witness_period: u64,
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
		witness_period: u64,
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
		Self { rpc_retry_client, chain_name, witness_period }
	}
}

impl TronRetryRpcClient<TronRpcClient> {
       pub async fn new(
	       scope: &Scope<'_, anyhow::Error>,
	       nodes: NodeContainer<TronEndpoints>,
	       expected_chain_id: U256,
	       tron_rpc_client_name: &'static str,
	       chain_name: &'static str,
	       witness_period: u64,
       ) -> Result<Self> {
	       let rpc_client_fut = TronRpcClient::new(
		       nodes.primary.http_endpoint.clone(),
		       nodes.primary.json_rpc_endpoint.clone(),
		       expected_chain_id.as_u64(),
		       chain_name,
	       )?;
	       let rpc_client = rpc_client_fut.await;

	       let backup_rpc_client = match &nodes.backup {
		       Some(ep) => {
			       let fut = TronRpcClient::new(
				       ep.http_endpoint.clone(),
				       ep.json_rpc_endpoint.clone(),
				       expected_chain_id.as_u64(),
				       chain_name,
			       )?;
			       Some(fut.await)
		       },
		       None => None,
	       };

	       Ok(Self::from_inner_clients(
		       scope,
		       rpc_client,
		       backup_rpc_client,
		       tron_rpc_client_name,
		       chain_name,
		       witness_period,
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
	       witness_period: u64,
	       private_key_file: std::path::PathBuf,
       ) -> Result<Self> {
	       let rpc_client_fut = TronRpcSigningClient::new(
		       private_key_file.clone(),
		       nodes.primary.http_endpoint.clone(),
		       nodes.primary.json_rpc_endpoint.clone(),
		       expected_chain_id.as_u64(),
		       chain_name,
	       )?;
	       let rpc_client = rpc_client_fut.await;

	       let backup_rpc_client = match &nodes.backup {
		       Some(ep) => {
			       let fut = TronRpcSigningClient::new(
				       private_key_file.clone(),
				       ep.http_endpoint.clone(),
				       ep.json_rpc_endpoint.clone(),
				       expected_chain_id.as_u64(),
				       chain_name,
			       )?;
			       Some(fut.await)
		       },
		       None => None,
	       };

	       Ok(Self::from_inner_clients(scope, rpc_client, backup_rpc_client, tron_rpc_client_name, chain_name, witness_period))
       }
}

#[async_trait::async_trait]
pub trait TronRetryRpcApi: Clone {
	// Tron HTTP API methods
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> TransactionInfo;
	async fn get_transaction_by_id(&self, tx_id: &str) -> TronTransaction;
	async fn get_block_balances(&self, block_number: BlockNumber, hash: &str) -> BlockBalance;
	async fn broadcast_hex(&self, transaction_hex: &str) -> serde_json::Value;
	async fn trigger_smart_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> UnsignedTronTransaction;

	// EVM-compatible JSON-RPC methods (via Tron's JSON-RPC)
	async fn chain_id(&self) -> U256;
	async fn get_logs(&self, filter: Filter) -> Vec<Log>;
	async fn transaction_receipt(&self, tx_hash: H256) -> TransactionReceipt;
	async fn block(&self, block_number: U64) -> Block<H256>;
	async fn block_by_hash(&self, block_hash: H256) -> Block<H256>;
	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction>;
	async fn get_transaction(&self, tx_hash: H256) -> Transaction;
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

	async fn get_transaction_by_id(&self, tx_id: &str) -> TronTransaction {
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

	async fn broadcast_hex(&self, transaction_hex: &str) -> serde_json::Value {
		let transaction_hex = transaction_hex.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new("broadcastHex".to_string(), Some(format!("{:?}", transaction_hex))),
				Box::pin(move |client| {
					let transaction_hex = transaction_hex.clone();
					Box::pin(async move { client.broadcast_hex(&transaction_hex).await })
				}),
			)
			.await
	}

	async fn trigger_smart_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> UnsignedTronTransaction {
		self.rpc_retry_client
			.request(
				RequestLog::new("triggerSmartContract".to_string(), Some(format!("{:?}", request))),
				Box::pin(move |client| {
					let request = request.clone();
					Box::pin(async move { client.trigger_smart_contract(request).await })
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

	async fn block_with_txs(&self, block_number: U64) -> Block<Transaction> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"eth_getBlockByNumber".to_string(),
					Some(format!("{:?} (with txs)", block_number)),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Transaction {
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
						client
							.send_transaction(TronTransactionRequest {
								owner_address: signer_address,
								contract_address,
								function_selector: std::str::from_utf8(
									&transaction.function_selector,
								)?
								.to_string(),
								parameter: transaction.data,
								fee_limit: {
									// This should never happen but just in case. We could consider
									// using u32 in the SC.
									if transaction.fee_limit > i64::MAX as u64 {
										return Err(anyhow::anyhow!(
											"fee_limit too large to fit in i64"
										));
									}
									transaction.fee_limit as i64
								},
								// value is automatically defaulted to zero
							})
							.await
					})
				}),
				MAX_BROADCAST_RETRIES,
			)
			.await
	}
}
