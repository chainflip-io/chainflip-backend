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

use anyhow::anyhow;
use ethers::types::{Block, Filter, Log, Transaction, TransactionReceipt, H256, U256, U64};
use futures_core::Future;
use reqwest::Client;
use serde_json::from_value;

use cf_utilities::redact_endpoint_secret::SecretUrl;

use super::rpc_client_api::{BlockBalance, BlockNumber, TransactionInfo, TronTransaction};
use crate::evm::rpc::{EvmRpcApi, EvmRpcClient};

// It is nice to separate the http and the json_rpc because some providers
// might not support both (e.g. TronGrid does not support JSON-RPC, only HTTP API
// in mainnet). This should hopefully achieve a better rpc provider diversity.
#[derive(Clone)]
pub struct TronRpcClient {
	// For the HTTP-API we need the historical balance query feature enabled for
	// the getBlockBalance query
	http_provider: Client,
	http_endpoint: SecretUrl,
	// Reuse the EVM RPC client for JSON-RPC, it's EVM-compatible
	evm_rpc_client: EvmRpcClient,
}

impl TronRpcClient {
	pub fn new(
		http_endpoint: SecretUrl,
		json_rpc_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = Self>> {
		let http_provider = Client::builder().build()?;

		// Create the EVM RPC client for JSON-RPC calls
		let evm_rpc_client_fut =
			EvmRpcClient::new(json_rpc_endpoint, expected_chain_id, chain_name)?;
		// TODO: Should we do some check for the http api endpoint? We require the node to be have
		// the historical balance query feature enabled. However, there doesn't seem to be a way
		// to query that more than actually making a getBlockBalance query and checking if it works.
		Ok(async move {
			let evm_rpc_client = evm_rpc_client_fut.await;

			Self { http_provider, http_endpoint, evm_rpc_client }
		})
	}

	/// Make a generic HTTP API call (for Tron's REST API)
	pub async fn call_http_api(
		&self,
		endpoint: &str,
		body: Option<serde_json::Value>,
	) -> anyhow::Result<serde_json::Value> {
		let url = format!("{}{}", self.http_endpoint.as_ref(), endpoint);

		let mut request = self.http_provider.post(&url);

		if let Some(body) = body {
			request = request.json(&body);
		}

		let response = request.send().await?;
		let json = response.json::<serde_json::Value>().await?;

		Ok(json)
	}
}

#[async_trait::async_trait]
pub trait TronRpcApi: Send + Sync + Clone + 'static {
	// HTTP API specific methods (Tron-specific, not available via JSON-RPC)
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo>;
	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction>;
	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance>;
}

// Implement EvmRpcApi for TronRpcClient to delegate JSON-RPC calls to the underlying EVM client
#[async_trait::async_trait]
impl EvmRpcApi for TronRpcClient {
	async fn estimate_gas(
		&self,
		req: &ethers::types::Eip1559TransactionRequest,
	) -> anyhow::Result<U256> {
		self.evm_rpc_client.estimate_gas(req).await
	}

	async fn get_logs(&self, filter: Filter) -> anyhow::Result<Vec<Log>> {
		self.evm_rpc_client.get_logs(filter).await
	}

	async fn chain_id(&self) -> anyhow::Result<U256> {
		self.evm_rpc_client.chain_id().await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt> {
		self.evm_rpc_client.transaction_receipt(tx_hash).await
	}

	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>> {
		self.evm_rpc_client.block(block_number).await
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>> {
		self.evm_rpc_client.block_by_hash(block_hash).await
	}

	async fn block_with_txs(&self, block_number: U64) -> anyhow::Result<Block<Transaction>> {
		self.evm_rpc_client.block_with_txs(block_number).await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: ethers::types::BlockNumber,
		reward_percentiles: &[f64],
	) -> anyhow::Result<ethers::types::FeeHistory> {
		self.evm_rpc_client
			.fee_history(block_count, newest_block, reward_percentiles)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<Transaction> {
		self.evm_rpc_client.get_transaction(tx_hash).await
	}

	async fn get_block_number(&self) -> anyhow::Result<U64> {
		self.evm_rpc_client.get_block_number().await
	}
}

#[async_trait::async_trait]
impl TronRpcApi for TronRpcClient {
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		let response = self
			.call_http_api("/gettransactioninfobyid", Some(serde_json::json!({"value": tx_id})))
			.await?;

		let transaction_info = from_value(response)
			.map_err(|err| anyhow!("Failed to parse transaction info: {}", err))?;

		Ok(transaction_info)
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction> {
		let response = self
			.call_http_api(
				"/gettransactionbyid",
				Some(serde_json::json!({"value": tx_id, "visible": false})),
			)
			.await?;

		let transaction =
			from_value(response).map_err(|err| anyhow!("Failed to parse transaction: {}", err))?;

		Ok(transaction)
	}

	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		let response = self
			.call_http_api(
				"/getblockbalance",
				Some(serde_json::json!({
					"number": block_number,
					"hash": hash,
					"visible": false
				})),
			)
			.await?;

		let block_balance = from_value(response)
			.map_err(|err| anyhow!("Failed to parse block balance data: {}", err))?;

		Ok(block_balance)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_chain_id() {
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await;

		let chain_id = tron_rpc_client.chain_id().await.unwrap();
		println!("Tron chain_id: {}", chain_id);
		println!("Tron chain_id (hex): 0x{:x}", chain_id);
		assert_eq!(chain_id, U256::from(3448148188u64));
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_get_transaction_info() {
		// Tron Nile testnet endpoints
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await;

		// Test getTransactionInfoById with a transaction ID
		let tx_id = "cbc9697b1ec1c6d0802631c82c411b083fcbb8297d6ddf88525e8378c6bd76f7";
		let result = tron_rpc_client.get_transaction_info_by_id(tx_id).await.unwrap();
		println!("Transaction info query result: {:#?}", result);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_get_transaction_by_id() {
		// Tron Nile testnet endpoints
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await;

		// Test getTransactionById with a transaction ID
		let tx_id = "7a89015e99a64e1731efe6da8ae705384a51592e38e715a0b045809b62ccd31d";
		let result = tron_rpc_client.get_transaction_by_id(tx_id).await.unwrap();
		println!("Transaction query result: {:#?}", result);

		// Verify the transaction ID matches
		assert_eq!(result.tx_id, tx_id);

		// Verify raw_data.data exists and contains the expected hex string
		assert_eq!(result.raw_data.data, Some("48656c6c6f20436861696e666c69702100".to_string()));
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_get_block_balances() {
		// Using qucknode because we need a node with historical balance query enabled
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://docs-demo.tron-mainnet.quiknode.pro/wallet".to_string()),
			SecretUrl::from("https://docs-demo.tron-mainnet.quiknode.pro/jsonrpc".to_string()),
			728126428, // Mainnet  chain ID (0x2b6653dc)
			"Tron",
		)
		.unwrap()
		.await;

		// Test getBlockBalance with block number and hash
		let block_num = 80079354;
		let block_hash = "0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945";
		let balance = tron_rpc_client.get_block_balances(block_num, block_hash).await.unwrap();
		println!("Block balance query result: {:?}", balance);
	}
}
