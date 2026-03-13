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
use ethers::{
	signers::{LocalWallet, Signer},
	types::{Block, Filter, Log, TransactionReceipt, H160, H256, U256, U64},
};
use futures_core::Future;
use reqwest::Client;
use serde_json::from_value;
use sp_core::ecdsa::Signature;
use std::{path::PathBuf, str::FromStr};

use cf_utilities::{read_clean_and_decode_hex_str_file, redact_endpoint_secret::SecretUrl};

use super::rpc_client_api::{
	BlockBalance, BlockNumber, BroadcastResponse, EstimateEnergyResult, Transaction,
	TransactionExtention, TransactionInfo, TriggerConstantContractRequest,
	TriggerSmartContractRequest, TronBlockRpc,
};
use crate::{
	evm::rpc::{EvmRpcApi, EvmRpcClient},
	rpc_utils,
	tron::rpc_client_api::TriggerConstantContractResult,
};
use anyhow::Context;

// It is nice to separate the http and the json_rpc because some providers
// might not support both (e.g. TronGrid does not support JSON-RPC, only HTTP API
// in mainnet). This should hopefully achieve a better rpc provider diversity.
#[derive(Clone)]
pub struct TronRpcClient {
	// For the HTTP-API we need the historical balance query feature enabled for
	// the getBlockBalance query
	http_provider: Client,
	http_endpoint: SecretUrl,
	// Store the JSON-RPC endpoint for raw calls that bypass ethers deserialization
	json_rpc_endpoint: SecretUrl,
	// Reuse the EVM RPC client for JSON-RPC, it's EVM-compatible
	evm_rpc_client: EvmRpcClient,
}

impl TronRpcClient {
	pub fn new(
		http_endpoint: SecretUrl,
		json_rpc_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = anyhow::Result<Self>>> {
		let http_provider = Client::builder().build()?;

		// Create the EVM RPC client for JSON-RPC calls
		let json_rpc_endpoint_clone = json_rpc_endpoint.clone();
		let evm_rpc_client_fut =
			EvmRpcClient::new(json_rpc_endpoint, expected_chain_id, chain_name)?;
		Ok(async move {
			let evm_rpc_client = evm_rpc_client_fut.await;

			let client = Self {
				http_provider,
				http_endpoint,
				json_rpc_endpoint: json_rpc_endpoint_clone,
				evm_rpc_client,
			};

			// Verify the HTTP API node has the historical balance query feature enabled.
			let block_number = client
				.get_block_number()
				.await
				.context("Failed to get block number during startup check")?;
			let block = client
				.block(block_number)
				.await
				.context("Failed to get block during startup check")?;
			let block_hash = block.hash.context("Block has no hash during startup check")?;
			client
				.get_block_balances(block_number.low_u64() as i64, &format!("{:x}", block_hash))
				.await
				.context("HTTP API node does not support getBlockBalance — ensure the node has the historical balance query feature enabled")?;

			Ok(client)
		})
	}

	/// Make a raw JSON-RPC call bypassing ethers provider and its deserialization.
	async fn call_json_rpc(
		&self,
		method: &str,
		params: serde_json::Value,
	) -> anyhow::Result<serde_json::Value> {
		rpc_utils::call_rpc_raw(
			&self.http_provider,
			self.json_rpc_endpoint.as_ref(),
			method,
			Some(params),
		)
		.await
		.map_err(|e| anyhow!("JSON-RPC call failed: {}", e))
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

#[derive(Clone)]
pub struct TronRpcSigningClient {
	wallet: LocalWallet,
	rpc_client: TronRpcClient,
}

impl TronRpcSigningClient {
	pub fn new(
		private_key_file: PathBuf,
		http_endpoint: SecretUrl,
		json_rpc_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = anyhow::Result<Self>>> {
		let rpc_client_fut =
			TronRpcClient::new(http_endpoint, json_rpc_endpoint, expected_chain_id, chain_name)?;

		let wallet = read_clean_and_decode_hex_str_file(
			&private_key_file,
			format!("{chain_name} Private Key").as_str(),
			|key| ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new),
		)?;

		Ok(async move {
			let rpc_client = rpc_client_fut.await?;

			Ok(Self { wallet: wallet.with_chain_id(expected_chain_id), rpc_client })
		})
	}
}

#[async_trait::async_trait]
pub trait TronRpcApi: Send + Sync + Clone + 'static {
	// HTTP API specific methods (Tron-specific, not available via JSON-RPC)
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo>;
	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<Transaction>;
	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance>;
	async fn trigger_constant_contract(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<TriggerConstantContractResult>;
	async fn trigger_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<TransactionExtention>;
	async fn estimate_energy(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<EstimateEnergyResult>;
	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> anyhow::Result<BroadcastResponse>;
}

// Implement EvmRpcApi for TronRpcClient to delegate JSON-RPC calls to the underlying EVM client.
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
		let result = self
			.call_json_rpc(
				"eth_getBlockByNumber",
				serde_json::json!([format!("0x{:x}", block_number), false]),
			)
			.await?;
		let tron_block: TronBlockRpc<H256> =
			from_value(result).map_err(|err| anyhow!("Failed to parse TronBlockRpc: {}", err))?;
		Ok(tron_block.into_ethers_block())
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>> {
		let result = self
			.call_json_rpc(
				"eth_getBlockByHash",
				serde_json::json!([format!("{:?}", block_hash), false]),
			)
			.await?;
		let tron_block: TronBlockRpc<H256> =
			from_value(result).map_err(|err| anyhow!("Failed to parse TronBlockRpc: {}", err))?;
		Ok(tron_block.into_ethers_block())
	}

	async fn block_with_txs(
		&self,
		block_number: U64,
	) -> anyhow::Result<Block<ethers::types::Transaction>> {
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

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<ethers::types::Transaction> {
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

		// If the transaction exists but is not yet included or has low number of confirmations it
		// can  return an empty object. Erroring with a different message than the JSON convertion
		// for clarity.
		if let Some(obj) = response.as_object() {
			if obj.is_empty() {
				return Err(anyhow!(
					"Transaction info not available yet for tx_id: {}. The transaction may still be processing or does not exist.",
					tx_id
				));
			}
		}

		let transaction_info = from_value(response)
			.map_err(|err| anyhow!("Failed to parse transaction info: {}", err))?;

		Ok(transaction_info)
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<Transaction> {
		let response = self
			.call_http_api(
				"/gettransactionbyid",
				Some(serde_json::json!({"value": tx_id, "visible": false})),
			)
			.await?;

		// If the transaction exists but is not yet included or has low number of confirmations it
		// can  return an empty object. Erroring with a different message than the JSON convertion
		// for clarity.
		if let Some(obj) = response.as_object() {
			if obj.is_empty() {
				return Err(anyhow!(
					"Transaction info not available yet for tx_id: {}. The transaction may still be processing or does not exist.",
					tx_id
				));
			}
		}

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

	async fn trigger_constant_contract(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<TriggerConstantContractResult> {
		let body = serde_json::json!({
			"owner_address": request.owner_address,
			"contract_address": request.contract_address,
			"function_selector": request.function_selector,
			"parameter": hex::encode(&request.parameter),
			"visible": false
		});

		let response = self.call_http_api("/triggerconstantcontract", Some(body)).await?;

		println!("Trigger constant contract rpc response: {:?}", response);

		let result: super::rpc_client_api::TriggerConstantContractResult = from_value(response)
			.map_err(|err| anyhow!("Failed to parse transaction extension: {}", err))?;

		// This just checks that the estimation was successful, not whether the tx reverted.
		// That shall be checked by the caller if/when needed.
		result.result.ensure_success("Failed to trigger constant contract")?;

		Ok(result)
	}

	async fn trigger_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<TransactionExtention> {
		let body = serde_json::json!({
			"owner_address": request.owner_address,
			"contract_address": request.contract_address,
			"function_selector": request.function_selector,
			"parameter": hex::encode(&request.parameter),
			"fee_limit": request.fee_limit,
			"visible": false
		});

		let response = self.call_http_api("/triggersmartcontract", Some(body)).await?;

		println!("Trigger contract rpc response: {:?}", response);

		let result: TransactionExtention = from_value(response)
			.map_err(|err| anyhow!("Failed to parse trigger smart contract result: {}", err))?;

		result.result.ensure_success("Failed to trigger smart contract")?;

		Ok(result)
	}

	async fn estimate_energy(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<EstimateEnergyResult> {
		let body = serde_json::json!({
			"owner_address": request.owner_address,
			"contract_address": request.contract_address,
			"function_selector": request.function_selector,
			"parameter": hex::encode(&request.parameter),
			"visible": false
		});

		let response = self.call_http_api("/estimateenergy", Some(body)).await?;

		let result: EstimateEnergyResult = from_value(response)
			.map_err(|err| anyhow!("Failed to parse estimate energy result: {}", err))?;

		Ok(result)
	}

	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> anyhow::Result<BroadcastResponse> {
		// Convert signatures to hex strings
		let signature_strings: Vec<String> =
			signatures.iter().map(|sig| hex::encode(sig.0)).collect();

		let body = serde_json::json!({
			"txID": format!("{:x}", tx_id),
			"raw_data": raw_data,
			"raw_data_hex": raw_data_hex,
			"signature": signature_strings,
			"visible": false
		});

		let response = self.call_http_api("/broadcasttransaction", Some(body)).await?;

		println!("Broadcast rpc response: {:?}", response);

		let broadcast_response: BroadcastResponse = from_value(response)
			.map_err(|e| anyhow!("Failed to parse broadcast response: {}", e))?;

		Ok(broadcast_response)
	}
}

#[async_trait::async_trait]
pub trait TronSigningRpcApi: TronRpcApi {
	fn address(&self) -> H160;

	fn sign_raw_bytes(&self, bytes: Vec<u8>) -> anyhow::Result<Signature>;
}

// TronRpcSigningClient wraps a TronRpcClient, so it can also satisfy EvmRpcApi by delegation.
// This allows TronCachingClient<TronRpcSigningClient> to be constructed.
#[async_trait::async_trait]
impl EvmRpcApi for TronRpcSigningClient {
	async fn estimate_gas(
		&self,
		req: &ethers::types::Eip1559TransactionRequest,
	) -> anyhow::Result<U256> {
		self.rpc_client.estimate_gas(req).await
	}

	async fn get_logs(
		&self,
		filter: ethers::types::Filter,
	) -> anyhow::Result<Vec<ethers::types::Log>> {
		self.rpc_client.get_logs(filter).await
	}

	async fn chain_id(&self) -> anyhow::Result<U256> {
		self.rpc_client.chain_id().await
	}

	async fn transaction_receipt(
		&self,
		tx_hash: ethers::types::H256,
	) -> anyhow::Result<ethers::types::TransactionReceipt> {
		self.rpc_client.transaction_receipt(tx_hash).await
	}

	async fn block(
		&self,
		block_number: ethers::types::U64,
	) -> anyhow::Result<ethers::types::Block<ethers::types::H256>> {
		self.rpc_client.block(block_number).await
	}

	async fn block_by_hash(
		&self,
		block_hash: ethers::types::H256,
	) -> anyhow::Result<ethers::types::Block<ethers::types::H256>> {
		self.rpc_client.block_by_hash(block_hash).await
	}

	async fn block_with_txs(
		&self,
		block_number: ethers::types::U64,
	) -> anyhow::Result<ethers::types::Block<ethers::types::Transaction>> {
		self.rpc_client.block_with_txs(block_number).await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: ethers::types::BlockNumber,
		reward_percentiles: &[f64],
	) -> anyhow::Result<ethers::types::FeeHistory> {
		self.rpc_client.fee_history(block_count, newest_block, reward_percentiles).await
	}

	async fn get_transaction(
		&self,
		tx_hash: ethers::types::H256,
	) -> anyhow::Result<ethers::types::Transaction> {
		self.rpc_client.get_transaction(tx_hash).await
	}

	async fn get_block_number(&self) -> anyhow::Result<ethers::types::U64> {
		self.rpc_client.get_block_number().await
	}
}

#[async_trait::async_trait]
impl TronSigningRpcApi for TronRpcSigningClient {
	fn address(&self) -> H160 {
		self.wallet.address()
	}

	fn sign_raw_bytes(&self, bytes: Vec<u8>) -> anyhow::Result<Signature> {
		// Hash the raw data with SHA256 (TRON uses SHA256, not Keccak256)
		let hash = sp_core::hashing::sha2_256(&bytes);

		// Sign the hash using the wallet
		let signature = self
			.wallet
			.sign_hash(H256::from(hash))
			.context("Failed to sign message for {self.chain_name}")?;

		// TRON uses recoverable signatures (65 bytes: r(32) + s(32) + v(1))
		let mut sig_bytes = [0u8; 65];
		sig_bytes[0..32].copy_from_slice(&signature.r.to_big_endian());
		sig_bytes[32..64].copy_from_slice(&signature.s.to_big_endian());
		sig_bytes[64] = signature.v as u8;

		Ok(Signature::from(sig_bytes))
	}
}

#[async_trait::async_trait]
impl TronRpcApi for TronRpcSigningClient {
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		self.rpc_client.get_transaction_info_by_id(tx_id).await
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<Transaction> {
		self.rpc_client.get_transaction_by_id(tx_id).await
	}

	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		self.rpc_client.get_block_balances(block_number, hash).await
	}

	async fn trigger_constant_contract(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<TriggerConstantContractResult> {
		self.rpc_client.trigger_constant_contract(request).await
	}

	async fn trigger_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<TransactionExtention> {
		self.rpc_client.trigger_contract(request).await
	}

	async fn estimate_energy(
		&self,
		request: TriggerConstantContractRequest,
	) -> anyhow::Result<EstimateEnergyResult> {
		self.rpc_client.estimate_energy(request).await
	}

	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> anyhow::Result<BroadcastResponse> {
		self.rpc_client
			.broadcast_transaction(tx_id, raw_data, raw_data_hex, signatures)
			.await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tron::rpc_client_api::{TransactionResultStatus, TronAddress};

	#[ignore = "requires local private tron node"]
	#[tokio::test]
	async fn test_private_node_block() {
		// let tron_rpc_client = TronRpcClient::new(
		// 	SecretUrl::from("http://localhost:8090/wallet".to_string()),
		// 	SecretUrl::from("http://localhost:8555/jsonrpc".to_string()),
		// 	4271970548, // local private node chain ID
		// 	"Tron-Local",
		// )
		// .unwrap()
		// .await;

		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await
		.unwrap();

		let block_number = tron_rpc_client.get_block_number().await.unwrap();
		println!("Current block number: {}", block_number);

		// let block =
		// tron_rpc_client.get_block_header_by_num(block_number.as_u64()).await.unwrap();
		let block = tron_rpc_client.block(block_number).await.unwrap();

		println!("Block: {:#?}", block);
	}

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
		.await
		.unwrap();

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
			SecretUrl::from("http://localhost:8090/wallet".to_string()),
			SecretUrl::from("http://localhost:8555/jsonrpc".to_string()),
			4271970548, // local private node chain ID
			"Tron-Local",
		)
		.unwrap()
		.await
		.unwrap();

		// Test getTransactionInfoById with a transaction ID
		let tx_id = "0360ad178013e9722e689956cd64b7af3296b722325cbf3a5f3f1a97731aa297";
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
		.await
		.unwrap();

		// Test getTransactionById with a transaction ID
		let tx_id = "bd17efdc7bd30e3887a3af59454bbe219ba53a4d91040aae4c0948edda586c0f";
		let result = tron_rpc_client.get_transaction_by_id(tx_id).await.unwrap();
		println!("Transaction query result: {:#?}", result);

		// Verify the transaction ID matches
		assert_eq!(result.tx_id, tx_id.parse().unwrap());
		assert_eq!(result.status(), TransactionResultStatus::Success);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_get_note() {
		// Tron Nile testnet endpoints
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await
		.unwrap();

		// Test getTransactionById with a transaction ID
		let tx_id = "7a89015e99a64e1731efe6da8ae705384a51592e38e715a0b045809b62ccd31d";
		let result = tron_rpc_client.get_transaction_by_id(tx_id).await.unwrap();
		println!("Transaction query result: {:#?}", result);

		// Verify the transaction ID matches
		assert_eq!(result.tx_id, tx_id.parse().unwrap());

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
		.await
		.unwrap();

		// Test getBlockBalance with block number and hash
		let block_num = 80079354;
		let block_hash = "0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945";
		let balance = tron_rpc_client.get_block_balances(block_num, block_hash).await.unwrap();
		println!("Block balance query result: {:?}", balance);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_nile_trigger_contract() {
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188,
			"Tron",
		)
		.unwrap()
		.await
		.unwrap();

		let unfunded_owner = TronAddress(
			hex::decode("41b7bd91a81449253dd0ee8c51c04e0578be6c4a91")
				.unwrap()
				.try_into()
				.unwrap(),
		);
		let funded_owner = TronAddress(
			hex::decode("41f10b2b8efd89cb89c3c43b54d628b4f15302233e")
				.unwrap()
				.try_into()
				.unwrap(),
		);
		let contract_address = TronAddress(
			hex::decode("41eca9bc828a3005b9a3b909f2cc5c2a54794de05f")
				.unwrap()
				.try_into()
				.unwrap(),
		);
		let function_selector = "transfer(address,uint256)".to_string();
		let parameter = hex::decode("00000000000000000000004115208EF33A926919ED270E2FA61367B2DA3753DA0000000000000000000000000000000000000000000000000000000000000032").unwrap();
		let fee_limit = 1000000000;

		// Test with unfunded account (expect failure)
		let constant_request_fail = TriggerConstantContractRequest {
			owner_address: unfunded_owner.clone(),
			contract_address: contract_address.clone(),
			function_selector: function_selector.clone(),
			parameter: parameter.clone(),
		};
		let simulation_fail =
			tron_rpc_client.trigger_constant_contract(constant_request_fail).await.unwrap();
		println!("Trigger constant contract (fail) {:?}", simulation_fail);
		assert!(simulation_fail.transaction.status() != TransactionResultStatus::Success);

		let trigger_request_fail = TriggerSmartContractRequest {
			owner_address: unfunded_owner,
			contract_address: contract_address.clone(),
			function_selector: function_selector.clone(),
			parameter: parameter.clone(),
			fee_limit,
		};
		let result_fail = tron_rpc_client.trigger_contract(trigger_request_fail).await.unwrap();
		println!("Trigger smart contract (fail) {:?}", result_fail);
		assert!(!result_fail.transaction.raw_data_hex.is_empty());
		assert!(result_fail.transaction.status() != TransactionResultStatus::Success);

		// Test with funded account (expect success)
		let constant_request_success = TriggerConstantContractRequest {
			owner_address: funded_owner.clone(),
			contract_address: contract_address.clone(),
			function_selector: function_selector.clone(),
			parameter: parameter.clone(),
		};
		let simulation_success = tron_rpc_client
			.trigger_constant_contract(constant_request_success)
			.await
			.unwrap();
		println!("Trigger constant contract (success) {:?}", simulation_success);
		assert_eq!(simulation_success.transaction.status(), TransactionResultStatus::Success);

		let trigger_request_success = TriggerSmartContractRequest {
			owner_address: funded_owner,
			contract_address,
			function_selector,
			parameter,
			fee_limit: 1,
		};
		let result_success =
			tron_rpc_client.trigger_contract(trigger_request_success).await.unwrap();
		println!("Trigger smart contract (success) {:?}", result_success);
		assert!(!result_success.transaction.raw_data_hex.is_empty());
		assert!(
			result_success.transaction.status() == TransactionResultStatus::Success ||
				result_success.transaction.status() == TransactionResultStatus::Unknown
		);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_estimate_energy() {
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from(
				"https://tron-mainnet.core.chainstack.com/95e61622bf6a8af293978377718e3b77/wallet"
					.to_string(),
			),
			SecretUrl::from(
				"https://tron-mainnet.core.chainstack.com/95e61622bf6a8af293978377718e3b77/jsonrpc"
					.to_string(),
			),
			728126428, // Mainnet chain ID
			"Tron",
		)
		.unwrap()
		.await
		.unwrap();

		// THPvaUhoh2Qn2y9THCZML3H815hhFhn5YC
		let owner_address = TronAddress(
			hex::decode("414578065e2a26889bfab1da855e1c3268c7a10f2c")
				.unwrap()
				.try_into()
				.unwrap(),
		);
		// TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t (mainnet USDT)
		let contract_address = TronAddress(
			hex::decode("41a614f803b6fd780986a42c78ec9c7f77e6ded13c")
				.unwrap()
				.try_into()
				.unwrap(),
		);

		let request = TriggerConstantContractRequest {
			owner_address,
			contract_address,
			function_selector: "balanceOf(address)".to_string(),
			parameter: hex::decode(
				"000000000000000000000000a614f803b6fd780986a42c78ec9c7f77e6ded13c",
			)
			.unwrap(),
		};

		let result = tron_rpc_client.estimate_energy(request).await.unwrap();
		println!("Estimate energy result: {:?}", result);
		assert!(result.energy_required.unwrap() > 0);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_trigger_contract() {
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("http://localhost:8090/wallet".to_string()),
			SecretUrl::from("http://localhost:8555/jsonrpc".to_string()),
			4271970548, // local private node chain ID
			"Tron-Local",
		)
		.unwrap()
		.await
		.unwrap();

		let request = TriggerSmartContractRequest {
			owner_address: TronAddress(
				hex::decode("41076d3803349fd5fb48863c5fc33483cb2243c0df")
					.unwrap()
					.try_into()
					.unwrap(),
			),
			contract_address: TronAddress(
				hex::decode("41d754a07f437c275457686cbf0c6eb7d28b0dadbd")
					.unwrap()
					.try_into()
					.unwrap(),
			),
			function_selector: "allBatch((uint256,uint256,address),(bytes32,address)[],(address,address)[],(address,address,uint256)[])".to_string(),
			parameter: hex::decode("35236817dbd2c0614726e46142ee3404955afd92284ed98061b17dda09c9fb860000000000000000000000000000000000000000000000000000000000000001000000000000000000000000a558b33409044a9802c0b94680816ce8dbdab07400000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000000000018000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap(),
			fee_limit: 50_000_000,
		};

		let result = tron_rpc_client.trigger_contract(request).await.unwrap();
		println!("Trigger smart contract result: {:?}", result);
		assert!(!result.transaction.raw_data_hex.is_empty());
	}

	#[ignore = "requires local private tron node"]
	#[tokio::test]
	async fn test_local_get_block_balance() {
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("http://localhost:8090/wallet".to_string()),
			SecretUrl::from("http://localhost:8555/jsonrpc".to_string()),
			4271970548, // local private node chain ID
			"Tron-Local",
		)
		.unwrap()
		.await
		.unwrap();

		let block_number = tron_rpc_client.get_block_number().await.unwrap();
		println!("Current block number: {}", block_number);

		// Get the block via JSON-RPC to obtain its hash
		let block = tron_rpc_client.block(block_number).await.unwrap();
		let block_hash = block.hash.expect("Block should have a hash");
		println!("Block hash: {:?}", block_hash);
		println!("Block hash: {:x}", block_hash);

		// Query block balances using the HTTP API
		let balance = tron_rpc_client
			.get_block_balances(block_number.low_u64() as i64, &format!("{:x}", block_hash))
			.await
			.unwrap();
		println!("Block balance: {:?}", balance);
	}
}
