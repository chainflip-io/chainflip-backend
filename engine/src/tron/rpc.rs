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
	types::{Block, Filter, Log, Transaction, TransactionReceipt, H160, H256, U256, U64},
};
use futures_core::Future;
use reqwest::Client;
use serde_json::from_value;
use sp_core::ecdsa::Signature;
use std::{path::PathBuf, str::FromStr};

use cf_utilities::{read_clean_and_decode_hex_str_file, redact_endpoint_secret::SecretUrl};

use super::rpc_client_api::{
	BlockBalance, BlockNumber, BroadcastResponse, TransactionInfo, TriggerSmartContractRequest,
	TronTransaction, TronTransactionRequest, UnsignedTronTransaction,
};
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

#[derive(Clone)]
pub struct TronRpcSigningClient {
	wallet: LocalWallet,
	rpc_client: TronRpcClient,
	chain_name: &'static str,
}

impl TronRpcSigningClient {
	pub fn new(
		private_key_file: PathBuf,
		http_endpoint: SecretUrl,
		json_rpc_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = Self>> {
		let rpc_client_fut =
			TronRpcClient::new(http_endpoint, json_rpc_endpoint, expected_chain_id, chain_name)?;

		let wallet = read_clean_and_decode_hex_str_file(
			&private_key_file,
			format!("{chain_name} Private Key").as_str(),
			|key| ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new),
		)?;

		Ok(async move {
			let rpc_client = rpc_client_fut.await;

			Self { wallet: wallet.with_chain_id(expected_chain_id), rpc_client, chain_name }
		})
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
	async fn broadcast_hex(&self, transaction_hex: &str) -> anyhow::Result<serde_json::Value>;
	async fn trigger_smart_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<UnsignedTronTransaction>;
	async fn broadcast_transaction(
		&self,
		tx_id: H256,
		raw_data: serde_json::Value,
		raw_data_hex: String,
		signatures: Vec<Signature>,
	) -> anyhow::Result<BroadcastResponse>;
}

// Implement EvmRpcApi for TronRpcClient to delegate JSON-RPC calls to the underlying EVM client.
// TODO: We might want to change or error out for the calls that don't work or make no sense for
// TRON.
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

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction> {
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

	async fn broadcast_hex(&self, transaction_hex: &str) -> anyhow::Result<serde_json::Value> {
		let response = self
			.call_http_api(
				"/broadcasthex",
				Some(serde_json::json!({
					"transaction": transaction_hex
				})),
			)
			.await?;

		Ok(response)
	}

	async fn trigger_smart_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<UnsignedTronTransaction> {
		// Build the request body explicitly with visible set to false
		// call_value, call_token_value and token_id will never be needed
		let body = serde_json::json!({
			"owner_address": request.owner_address,
			"contract_address": request.contract_address,
			"function_selector": request.function_selector,
			"parameter": hex::encode(&request.parameter),
			"fee_limit": request.fee_limit,
			"visible": false
		});

		let response = self.call_http_api("/triggersmartcontract", Some(body)).await?;

		let transaction_extension: super::rpc_client_api::TransactionExtention =
			from_value(response)
				.map_err(|err| anyhow!("Failed to parse transaction extension: {}", err))?;

		// Check if the result is successful
		if !transaction_extension.result.result {
			return Err(anyhow!("Failed to trigger smart contract: result is false"));
		}

		let tx = transaction_extension.transaction;

		Ok(UnsignedTronTransaction {
			tx_id: tx.tx_id,
			raw_data_hex: tx.raw_data_hex,
			raw_data: serde_json::to_value(&tx.raw_data)
				.map_err(|e| anyhow!("Failed to serialize raw_data: {}", e))?,
		})
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

		let broadcast_response: BroadcastResponse = from_value(response)
			.map_err(|e| anyhow!("Failed to parse broadcast response: {}", e))?;

		Ok(broadcast_response)
	}
}

#[async_trait::async_trait]
pub trait TronSigningRpcApi: TronRpcApi {
	fn address(&self) -> H160;

	async fn send_transaction(&self, tx: TronTransactionRequest) -> anyhow::Result<H256>;
}

#[async_trait::async_trait]
impl TronRpcApi for TronRpcSigningClient {
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		self.rpc_client.get_transaction_info_by_id(tx_id).await
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction> {
		self.rpc_client.get_transaction_by_id(tx_id).await
	}

	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		self.rpc_client.get_block_balances(block_number, hash).await
	}

	async fn broadcast_hex(&self, transaction_hex: &str) -> anyhow::Result<serde_json::Value> {
		self.rpc_client.broadcast_hex(transaction_hex).await
	}

	async fn trigger_smart_contract(
		&self,
		request: TriggerSmartContractRequest,
	) -> anyhow::Result<UnsignedTronTransaction> {
		self.rpc_client.trigger_smart_contract(request).await
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

#[async_trait::async_trait]
impl TronSigningRpcApi for TronRpcSigningClient {
	fn address(&self) -> H160 {
		self.wallet.address()
	}

	async fn send_transaction(&self, tx: TronTransactionRequest) -> anyhow::Result<H256> {
		// Build the trigger smart contract request
		let trigger_request = TriggerSmartContractRequest {
			owner_address: tx.owner_address,
			contract_address: tx.contract_address,
			function_selector: tx.function_selector,
			parameter: tx.parameter,
			fee_limit: tx.fee_limit,
		};

		// Get the unsigned transaction from trigger_smart_contract
		// TODO: TBD if we want to check some of the returned transaction data
		// to ensure that the rpc is not returning some malicious values. We
		// could check the fee_limit, owner_address and some few mmore values.
		// However, we can't really check everything unless we do the same
		// process locally so it might be ok to just trust the rpc.
		let unsigned_tx = self.trigger_smart_contract(trigger_request).await?;

		// Decode the raw_data_hex to bytes
		let raw_data_bytes = hex::decode(&unsigned_tx.raw_data_hex)
			.map_err(|e| anyhow!("Failed to decode raw_data_hex: {}", e))?;

		// Hash the raw data with SHA256 (TRON uses SHA256, not Keccak256)
		let hash = sp_core::hashing::sha2_256(&raw_data_bytes);

		// Sign the hash using the wallet
		let signature = self.wallet.sign_hash(H256::from(hash))?;

		// Convert ethers signature to sp_core::ecdsa::Signature
		// TRON uses recoverable signatures (65 bytes: r(32) + s(32) + v(1))
		let mut sig_bytes = [0u8; 65];
		signature.r.to_big_endian(&mut sig_bytes[0..32]);
		signature.s.to_big_endian(&mut sig_bytes[32..64]);
		sig_bytes[64] = signature.v as u8;

		let signature = Signature::from(sig_bytes);

		// Broadcast the signed transaction
		let response = self
			.broadcast_transaction(
				unsigned_tx.tx_id,
				unsigned_tx.raw_data,
				unsigned_tx.raw_data_hex,
				vec![signature],
			)
			.await?;

		// Check if the broadcast was successful
		if !response.result {
			let error_message = response.message.as_deref().unwrap_or("Unknown error");
			let error_code =
				response.code.as_deref().map(|c| format!(" (code: {})", c)).unwrap_or_default();
			return Err(anyhow!("Transaction broadcast failed: {}{}", error_message, error_code));
		}

		// The transaction ID is already in the unsigned_tx
		Ok(unsigned_tx.tx_id)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tron::rpc_client_api::TronAddress;

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
		let tx_id = "bd17efdc7bd30e3887a3af59454bbe219ba53a4d91040aae4c0948edda586c0f";
		let result = tron_rpc_client.get_transaction_by_id(tx_id).await.unwrap();
		println!("Transaction query result: {:#?}", result);

		// Verify the transaction ID matches
		assert_eq!(result.tx_id, tx_id.parse().unwrap());
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
		.await;

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
		.await;

		// Test getBlockBalance with block number and hash
		let block_num = 80079354;
		let block_hash = "0000000004c5e9fa0b5bff64330976a20f1e5007f66f3f0524168a782d998945";
		let balance = tron_rpc_client.get_block_balances(block_num, block_hash).await.unwrap();
		println!("Block balance query result: {:?}", balance);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_tron_trigger_smart_contract() {
		// Tron Nile testnet endpoints
		let tron_rpc_client = TronRpcClient::new(
			SecretUrl::from("https://docs-demo.tron-mainnet.quiknode.pro/wallet".to_string()),
			SecretUrl::from("https://docs-demo.tron-mainnet.quiknode.pro/jsonrpc".to_string()),
			728126428,
			"Tron",
		)
		.unwrap()
		.await;

		// Create a request to trigger a smart contract (USDT transfer example)
		let request = TriggerSmartContractRequest {
			owner_address: TronAddress(
						hex::decode("41b7bd91a81449253dd0ee8c51c04e0578be6c4a91")
							.unwrap()
							.try_into()
							.unwrap(),
					),
			contract_address: TronAddress(
						hex::decode("41a614f803b6fd780986a42c78ec9c7f77e6ded13c")
							.unwrap()
							.try_into()
							.unwrap(),
					),
			function_selector: "transfer(address,uint256)".to_string(),
			parameter: hex::decode("00000000000000000000004115208EF33A926919ED270E2FA61367B2DA3753DA0000000000000000000000000000000000000000000000000000000000000032").unwrap(),
			fee_limit: 1000000000,
		};

		let unsigned_tx = tron_rpc_client.trigger_smart_contract(request).await.unwrap();
		println!("Trigger smart contract {:?}", unsigned_tx);
		println!("Trigger smart contract result (tx_id): {:x}", unsigned_tx.tx_id);
		println!(
			"Trigger smart contract result (raw_data_hex length): {}",
			unsigned_tx.raw_data_hex.len()
		);

		// Verify we got valid data
		assert!(!unsigned_tx.raw_data_hex.is_empty());
		assert!(unsigned_tx.raw_data_hex.len() > 0);
	}

	#[ignore = "requires access to external RPC and private key"]
	#[tokio::test]
	async fn test_tron_send_transaction() {
		// Fill in the path to your private key file
		let private_key_file =
			PathBuf::from("/home/albert/work/backend_tron/chainflip-backend/tron_private_key");

		// Tron Mainnet endpoints
		let tron_signing_client = TronRpcSigningClient::new(
			private_key_file,
			SecretUrl::from("https://nile.trongrid.io/wallet".to_string()),
			SecretUrl::from("https://nile.trongrid.io/jsonrpc".to_string()),
			3448148188, // Nile testnet chain ID (0xcd8690dc)
			"Tron-Nile",
		)
		.unwrap()
		.await;

		println!("Signer address: {:x}", tron_signing_client.address());

		// Create a transaction request to transfer USDT (same data as
		// test_tron_trigger_smart_contract)
		let tx_request = TronTransactionRequest {
            owner_address: TronAddress(
                hex::decode("41f10b2b8efd89cb89c3c43b54d628b4f15302233e")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            contract_address: TronAddress(
                hex::decode("41eca9bc828a3005b9a3b909f2cc5c2a54794de05f")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            ),
            function_selector: "transfer(address,uint256)".to_string(),
            parameter: hex::decode(
                "00000000000000000000004115208EF33A926919ED270E2FA61367B2DA3753DA0000000000000000000000000000000000000000000000000000000000000032"
            )
            .unwrap(),
            fee_limit: 1000000000,
        };

		// Send the transaction (encode + sign + broadcast)
		let tx_hash = tron_signing_client.send_transaction(tx_request).await.unwrap();

		println!("Transaction sent successfully!");
		println!("Transaction hash: {:x}", tx_hash);

		// We need to wait for the transaction to be included
		println!("Waiting for transaction to be processed...");
		tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;

		// Verify the transaction was broadcast by fetching its info
		let tx_info = tron_signing_client
			.get_transaction_info_by_id(&format!("{:x}", tx_hash))
			.await
			.map_err(|e| {
				eprintln!("Failed to get transaction info: {}", e);
				e
			})
			.unwrap();

		println!("Transaction info: {:#?}", tx_info);
	}
}
