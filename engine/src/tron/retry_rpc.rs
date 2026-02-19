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
	retrier::{RequestLog, RetrierClient, MAX_RPC_RETRY_DELAY},
	settings::{NodeContainer, TronEndpoints},
};
// use cf_chains::Tron;
use cf_utilities::task_scope::Scope;
use core::time::Duration;
use ethers::types::{Block, Filter, Log, Transaction, TransactionReceipt, H256, U256, U64};

use anyhow::Result;

use super::{
	rpc::{TronRpcApi, TronRpcClient},
	rpc_client_api::{
		BlockBalance, BlockNumber, BroadcastResponse, TransactionInfo, TriggerSmartContractRequest,
		TronTransaction, UnsignedTronTransaction,
	},
};
use sp_core::ecdsa::Signature;

#[derive(Clone)]
pub struct TronRetryRpcClient {
	rpc_retry_client: RetrierClient<TronRpcClient>,
	chain_name: &'static str,
	witness_period: u64,
}

const TRON_RPC_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

impl TronRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<TronEndpoints>,
		expected_chain_id: u64,
		chain_name: &'static str,
		witness_period: u64,
	) -> Result<Self> {
		let primary = {
			let http = nodes.primary.http_endpoint.clone();
			let json = nodes.primary.json_rpc_endpoint.clone();
			TronRpcClient::new(http, json, expected_chain_id, chain_name)?
		};
		let backup = if let Some(backup_endpoint) = nodes.backup.clone() {
			Some(TronRpcClient::new(
				backup_endpoint.http_endpoint,
				backup_endpoint.json_rpc_endpoint,
				expected_chain_id,
				chain_name,
			)?)
		} else {
			None
		};
		let rpc_retry_client = RetrierClient::new(
			scope,
			"tron_rpc",
			primary,
			backup,
			TRON_RPC_TIMEOUT,
			MAX_RPC_RETRY_DELAY,
			MAX_CONCURRENT_SUBMISSIONS,
		);
		Ok(Self { rpc_retry_client, chain_name, witness_period })
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
	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> BroadcastResponse;

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
impl TronRetryRpcApi for TronRetryRpcClient {
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

	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> BroadcastResponse {
		self.rpc_retry_client
			.request(
				RequestLog::new("broadcastTransaction".to_string(), Some(format!("{:?}", tx_id))),
				Box::pin(move |client| {
					let raw_data = raw_data.clone();
					let raw_data_hex = raw_data_hex.clone();
					let signatures = signatures.clone();
					Box::pin(async move {
						client
							.broadcast_transaction(tx_id, raw_data, raw_data_hex, signatures)
							.await
					})
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
