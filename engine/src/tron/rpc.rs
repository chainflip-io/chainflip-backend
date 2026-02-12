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
use futures_core::Future;
use reqwest::Client;
use serde_json::from_value;

use cf_utilities::{make_periodic_tick, redact_endpoint_secret::SecretUrl};

use super::rpc_client_api::{BlockBalance, TransactionInfo};
use crate::{constants::RPC_RETRY_CONNECTION_INTERVAL, rpc_utils::Error};

// It is nice to separate the http and the json_rpc because some providers
// might not support both (e.g. TronGrid does not support JSON-RPC, only HTTP API
// in mainnet). This should hopefully achieve a better rpc provider diversity.
#[derive(Clone)]
pub struct TronRpcClient {
	// For the HTTP-API we need the historical balance query feature enabled for
	// the getBlockBalance query
	http_provider: Client,
	json_rpc_provider: Client,
	http_endpoint: SecretUrl,
	json_rpc_endpoint: SecretUrl,
	chain_name: &'static str,
}

impl TronRpcClient {
	pub fn new(
		http_endpoint: SecretUrl,
		json_rpc_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = Self>> {
		let http_provider = Client::builder().build()?;
		let json_rpc_provider = Client::builder().build()?;

		let client = TronRpcClient {
			http_provider,
			json_rpc_provider,
			http_endpoint: http_endpoint.clone(),
			json_rpc_endpoint,
			chain_name,
		};

		Ok(async move {
			// We don't want to return an error here. Returning an error means that we'll exit the
			// CFE. So on client creation we wait until we can be successfully connected to this
			// Tron node. So the other chains are unaffected
			let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
			loop {
				poll_interval.tick().await;
				match client.chain_id().await {
					Ok(chain_id) if chain_id == expected_chain_id => break client,
					Ok(chain_id) => {
						tracing::error!(
							"Connected to {chain_name} node but with incorrect chain_id {chain_id}, expected {expected_chain_id} from {http_endpoint}. \
							Please check your CFE configuration file...",
						);
					},
					Err(e) => tracing::error!(
						"Cannot connect to a {chain_name:?} node at {http_endpoint} with error: {e}. \
						Please check your CFE configuration file. Retrying in {:?}...",
						poll_interval.period()
					),
				}
			}
		})
	}

	/// Make a generic JSON-RPC call
	pub async fn call_rpc(
		&self,
		method: &str,
		params: Option<serde_json::Value>,
	) -> Result<serde_json::Value, Error> {
		crate::rpc_utils::call_rpc_raw(
			&self.json_rpc_provider,
			self.json_rpc_endpoint.as_ref(),
			method,
			params,
		)
		.await
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
	async fn chain_id(&self) -> anyhow::Result<u64>;
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo>;
	async fn get_block_balances(
		&self,
		block_number: u64,
		hash: &str,
	) -> anyhow::Result<BlockBalance>;
}

#[async_trait::async_trait]
impl TronRpcApi for TronRpcClient {
	async fn chain_id(&self) -> anyhow::Result<u64> {
		let result = self.call_rpc("eth_chainId", None).await?;

		// The result is a hex string like "0x2b6653dc"
		let chain_id_hex = result
			.as_str()
			.ok_or_else(|| anyhow::anyhow!("chain_id response was not a string"))?;

		// Remove "0x" prefix if present and parse as hex
		let chain_id_hex = chain_id_hex.strip_prefix("0x").unwrap_or(chain_id_hex);
		let chain_id = u64::from_str_radix(chain_id_hex, 16)?;

		Ok(chain_id)
	}

	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		let response = self
			.call_http_api("/gettransactioninfobyid", Some(serde_json::json!({"value": tx_id})))
			.await?;

		let transaction_info = from_value(response)
			.map_err(|err| anyhow!("Failed to parse transaction info: {}", err))?;

		Ok(transaction_info)
	}

	async fn get_block_balances(
		&self,
		block_number: u64,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		let response = self
			.call_http_api(
				"/getblockbalance",
				Some(serde_json::json!({
					"number": block_number,
					"hash": hash,
					"visible": true
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
		assert_eq!(chain_id, 3448148188);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_http_api() {
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
	async fn test_tron_get_block_balances() {
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
