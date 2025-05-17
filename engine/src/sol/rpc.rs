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

use futures_core::Future;

use reqwest::{header::CONTENT_TYPE, Client};

use serde_json::{from_value, json};

use cf_utilities::make_periodic_tick;
use tracing::error;

use crate::{btc::rpc::Error, constants::RPC_RETRY_CONNECTION_INTERVAL};
use cf_utilities::redact_endpoint_secret::SecretUrl;

use anyhow::{anyhow, Result};
use tracing::warn;

use cf_chains::sol::{SolAddress, SolHash, SolSignature};

use super::{commitment_config::CommitmentConfig, rpc_client_api::*};
use std::str::FromStr;

#[derive(Clone)]
pub struct SolRpcClient {
	// Internally the Client is Arc'd
	client: Client,
	endpoint: SecretUrl,
}

impl SolRpcClient {
	pub fn new(
		endpoint: SecretUrl,
		expected_genesis_hash: Option<SolHash>,
	) -> anyhow::Result<impl Future<Output = Self>> {
		let client = Client::builder().build()?;

		Ok(async move {
			// We don't want to return an error here. Returning an error means that we'll exit the
			// CFE. So on client creation we wait until we can be successfully connected to the
			// Solana node. So the other chains are unaffected
			let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
			loop {
				poll_interval.tick().await;
				match expected_genesis_hash {
					None => {
						warn!("Skipping Solana genesis hash check");
						break;
					},
					Some(expected_hash) => match get_genesis_hash(&client, &endpoint).await {
						Ok(genesis_hash) =>
							if expected_hash == genesis_hash {
								break;
							} else {
								error!(
								"Connected to Solana node at {0} but the genesis hash {genesis_hash} does not match the expected genesis hash. Please check your CFE configuration file.",
								endpoint
							)
							},
						Err(e) => {
							tracing::error!(
								"Cannot connect to Solana node at {1} with error: {e}. \
											Please check your CFE configuration file. Retrying in {:?}...",
								poll_interval.period(),
								endpoint
							)
						},
					},
				}
			}
			Self { client, endpoint }
		})
	}

	async fn call_rpc(
		&self,
		method: &str,
		params: Option<serde_json::Value>,
	) -> Result<serde_json::Value, Error> {
		call_rpc_raw(&self.client, &self.endpoint, method, params).await
	}
}

async fn call_rpc_raw(
	client: &Client,
	endpoint: &SecretUrl,
	method: &str,
	params: Option<serde_json::Value>,
) -> Result<serde_json::Value, Error> {
	let request_body = json!({
		"jsonrpc": "2.0",
		"id": 0,
		"method": method,
		"params": params.unwrap_or_else(|| json!([]))
	});

	let response = client
		.post(endpoint.as_ref())
		.header(CONTENT_TYPE, "application/json")
		.json(&request_body)
		.send()
		.await
		.map_err(reqwest::Error::without_url)
		.map_err(Error::Transport)?;

	let mut json = response.json::<serde_json::Value>().await.map_err(Error::Transport)?;

	if json["error"].is_object() {
		return Err(Error::Rpc(serde_json::from_value(json["error"].clone()).map_err(Error::Json)?));
	}

	Ok(json["result"].take())
}

async fn get_genesis_hash(client: &Client, endpoint: &SecretUrl) -> anyhow::Result<SolHash> {
	let json_value = call_rpc_raw(client, endpoint, "getGenesisHash", None)
		.await
		.map_err(anyhow::Error::msg)?;

	let genesis_hash_str = json_value
		.as_str()
		.ok_or(anyhow!("Missing or empty `result` field in getGenesisHash response"))?;

	let genesis_hash =
		SolHash::from_str(genesis_hash_str).map_err(|_| anyhow!("Invalid genesis hash"))?;

	Ok(genesis_hash)
}

fn encode_pubkey(pubkey: &SolAddress) -> String {
	bs58::encode(pubkey).into_string()
}

#[async_trait::async_trait]
pub trait SolRpcApi {
	async fn get_block(
		&self,
		slot: u64,
		config: RpcBlockConfig,
	) -> anyhow::Result<UiConfirmedBlock>;
	async fn get_slot(&self, commitment: CommitmentConfig) -> anyhow::Result<u64>; // Slot
	async fn get_recent_prioritization_fees(&self) -> anyhow::Result<Vec<RpcPrioritizationFee>>;
	async fn get_multiple_accounts(
		&self,
		pubkeys: &[SolAddress],
		config: RpcAccountInfoConfig,
	) -> Result<Response<Vec<Option<UiAccount>>>>;
	async fn get_signature_statuses(
		&self,
		signatures: &[SolSignature],
	) -> Result<Response<Vec<Option<TransactionStatus>>>>;
	async fn get_transaction(
		&self,
		signature: &SolSignature,
		config: RpcTransactionConfig,
	) -> Result<EncodedConfirmedTransactionWithStatusMeta>;
	async fn send_transaction(
		&self,
		transaction: String,
		config: RpcSendTransactionConfig,
	) -> Result<SolSignature>;

	async fn simulate_transaction(
		&self,
		transaction: String,
		config: RpcSimulateTransactionConfig,
	) -> Result<Response<RpcSimulateTransactionResult>>;
}

#[async_trait::async_trait]
impl SolRpcApi for SolRpcClient {
	async fn get_block(
		&self,
		slot: u64,
		config: RpcBlockConfig,
	) -> anyhow::Result<UiConfirmedBlock> {
		let response = self.call_rpc("getBlock", Some(json!([slot, json!(config)]))).await?;
		let block: UiConfirmedBlock =
			from_value(response).map_err(|err| anyhow!("Failed to parse block {}", err))?;
		Ok(block)
	}

	async fn get_slot(&self, commitment: CommitmentConfig) -> anyhow::Result<u64> {
		let response = self.call_rpc("getSlot", Some(json!([json!(commitment)]))).await?;
		let slot: u64 =
			from_value(response).map_err(|err| anyhow!("Failed to parse block {}", err))?;
		Ok(slot)
	}

	async fn get_recent_prioritization_fees(&self) -> anyhow::Result<Vec<RpcPrioritizationFee>> {
		let response = self.call_rpc("getRecentPrioritizationFees", None).await?;
		let fees: Vec<RpcPrioritizationFee> = from_value(response)
			.map_err(|err| anyhow!("Failed to parse prioritization fees: {}", err))?;
		Ok(fees)
	}

	async fn get_multiple_accounts(
		&self,
		pubkeys: &[SolAddress],
		config: RpcAccountInfoConfig,
	) -> Result<Response<Vec<Option<UiAccount>>>> {
		let encoded_pubkeys: Vec<_> = pubkeys.iter().map(encode_pubkey).collect();

		let response = self
			.call_rpc("getMultipleAccounts", Some(json!([encoded_pubkeys, json!(config)])))
			.await?;

		let Response { context, value: accounts } =
			serde_json::from_value::<Response<Vec<Option<UiAccount>>>>(response.clone())?;
		Ok(Response { context, value: accounts })
	}

	async fn get_signature_statuses(
		&self,
		signatures: &[SolSignature],
	) -> Result<Response<Vec<Option<TransactionStatus>>>> {
		let response = self
			.call_rpc(
				"getSignatureStatuses",
				Some(json!([
					signatures,
					json!({
						"searchTransactionHistory": true
					})
				])),
			)
			.await?;
		let Response { context, value: tx_statuses } =
			serde_json::from_value::<Response<Vec<Option<TransactionStatus>>>>(response.clone())?;
		Ok(Response { context, value: tx_statuses })
	}

	async fn get_transaction(
		&self,
		signature: &SolSignature,
		config: RpcTransactionConfig,
	) -> anyhow::Result<EncodedConfirmedTransactionWithStatusMeta> {
		let response =
			self.call_rpc("getTransaction", Some(json!([signature, json!(config)]))).await?;

		let transaction_data = from_value(response)
			.map_err(|err| anyhow!("Failed to parse transaction data {}", err))?;

		Ok(transaction_data)
	}

	// Expecting a fully-signed transaction encoded as a string.
	async fn send_transaction(
		&self,
		transaction: String,
		config: RpcSendTransactionConfig,
	) -> Result<SolSignature> {
		let response = self
			.call_rpc("sendTransaction", Some(json!([transaction, json!(config)])))
			.await?;
		let signature: SolSignature = from_value(response)
			.map_err(|err| anyhow!("Failed to parse the resulting signature: {}", err))?;
		Ok(signature)
	}

	async fn simulate_transaction(
		&self,
		transaction: String,
		config: RpcSimulateTransactionConfig,
	) -> Result<Response<RpcSimulateTransactionResult>> {
		let response = self
			.call_rpc("simulateTransaction", Some(json!([transaction, json!(config)])))
			.await?;
		let Response { context, value: simulation_result } =
			serde_json::from_value::<Response<RpcSimulateTransactionResult>>(response.clone())?;

		Ok(Response { context, value: simulation_result })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use base64::{prelude::BASE64_STANDARD, Engine};

	#[test]
	fn test_encoding() {
		let pubkey = SolAddress::from_str("vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg").unwrap();
		let encoded = encode_pubkey(&pubkey);
		assert_eq!(encoded, "vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg");
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_sol_asyc() {
		let sol_rpc_client =
			SolRpcClient::new(SecretUrl::from("https://api.testnet.solana.com".to_string()), None)
				.unwrap()
				.await;

		get_genesis_hash(&sol_rpc_client.client, &sol_rpc_client.endpoint)
			.await
			.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_sol_devnet() {
		let sol_rpc_client = SolRpcClient::new(
			SecretUrl::from("https://api.devnet.solana.com".to_string()),
			Some(SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap()),
		)
		.unwrap()
		.await;

		let slot = sol_rpc_client.get_slot(CommitmentConfig::finalized()).await.unwrap();
		println!("slot: {:?}", slot);

		let priority_fees = sol_rpc_client.get_recent_prioritization_fees().await.unwrap();
		println!("priority_fees: {:?}", priority_fees);

		let result = sol_rpc_client
			.get_multiple_accounts(
				&[SolAddress::from_str("vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg").unwrap()],
				RpcAccountInfoConfig {
					encoding: Some(UiAccountEncoding::JsonParsed),
					data_slice: None,
					commitment: Some(CommitmentConfig::finalized()),
					min_context_slot: None,
				},
			)
			.await
			.unwrap();

		println!("rpc context: {:?}", result.context);
		println!("account_info: {:?}", result.value);

		let result: Response<Vec<Option<UiAccount>>> = sol_rpc_client
			.get_multiple_accounts(
				&[
					SolAddress::from_str("vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg").unwrap(),
					SolAddress::from_str("4fYNw3dojWmQ4dXtSGE9epjRGy9pFSx62YypT7avPYvA").unwrap(),
				],
				RpcAccountInfoConfig {
					encoding: Some(UiAccountEncoding::JsonParsed),
					data_slice: None,
					commitment: Some(CommitmentConfig::finalized()),
					min_context_slot: None,
				},
			)
			.await
			.unwrap();
		println!("account_info: {:?}", result.value);

		let block = sol_rpc_client
			.get_block(
				300620702,
				RpcBlockConfig {
					encoding: Some(UiTransactionEncoding::JsonParsed),
					transaction_details: Some(TransactionDetails::None),
					rewards: Some(false),
					commitment: Some(CommitmentConfig::finalized()),
					max_supported_transaction_version: None,
				},
			)
			.await
			.unwrap();
		println!("block: {:?}", block);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_sol_transaction() {
		let sol_rpc_client = SolRpcClient::new(
			SecretUrl::from("https://api.devnet.solana.com".to_string()),
			Some(SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap()),
		)
		.unwrap()
		.await;

		let signature = SolSignature::from_str("2Nb7bSQWoUYrEN6PYGN7Jhgs29HjSXEeM2mFKzkqwTiARM8EwXPQ6DMvQbvqLqxogXtvYtpxE44AsDeSS3e3fsDY").unwrap();

		let transaction = sol_rpc_client
			.get_transaction(
				&signature,
				RpcTransactionConfig {
					encoding: None,
					commitment: Some(CommitmentConfig::finalized()),
					max_supported_transaction_version: None,
				},
			)
			.await
			.unwrap();
		println!("transaction: {:?}", transaction);

		let signature_status = sol_rpc_client.get_signature_statuses(&[signature]).await.unwrap();

		let confirmation_status = signature_status
			.value
			.first()
			.and_then(Option::as_ref)
			.and_then(|ts| ts.confirmation_status.as_ref())
			.expect("Expected confirmation_status to be Some");

		println!("Signature status: {:?}", signature_status);
		assert_eq!(confirmation_status, &TransactionConfirmationStatus::Finalized);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_sol_simulate_transaction() {
		let sol_rpc_client = SolRpcClient::new(
			SecretUrl::from("https://api.devnet.solana.com".to_string()),
			Some(SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap()),
		)
		.unwrap()
		.await;

		// Serialized Versioned transaction from:
		// const connection = new Connection(clusterApiUrl("devnet"), "confirmed");
		// const chainlinkProgramId = new PublicKey(
		//   "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny"
		// );
		// const chainlinkFeed = new PublicKey(
		// 	// "Cv4T27XbjVoKUYwP72NQQanvZeA7W4YF9L4EnYT9kx5o" // BTC Mainnet
		// 	"6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe" // BTC Devnet
		// 	// "CzZQBrJCLqjXRfMjRN3fhbxur2QYHUzkpaRwkWsiPqbz" /// From docs example
		//   );

		// Stands for "sha256("global:latest_round_data")[0:8]"
		// const QUERY_INSTRUCTION_DISCRIMINATOR = Buffer.from([
		//   0x27, 0xfb, 0x82, 0x9f, 0x2e, 0x88, 0xa4, 0xa9,
		// ]);

		// // enum Query {
		// //     Version,
		// //     Decimals,
		// //     Description,
		// //     RoundData { round_id: u32 },
		// //     LatestRoundData,
		// //     Aggregator,
		// // }
		// const queryByte = Buffer.from([0x04]); // Adjust based on your Query enum variant

		// const instruction = new TransactionInstruction({
		//   programId: chainlinkProgramId,
		//   keys: [
		// 	{
		// 	  pubkey: chainlinkFeed,
		// 	  isSigner: false,
		// 	  isWritable: false,
		// 	},
		//   ],
		//   data: Buffer.concat([QUERY_INSTRUCTION_DISCRIMINATOR, queryByte]), // 8-byte
		// discriminator + query enun  });

		// const { blockhash } = await connection.getLatestBlockhash("confirmed");
		// const messageV0 = new TransactionMessage({
		//   payerKey: pg.wallet.publicKey,
		//   recentBlockhash: blockhash,
		//   instructions: [instruction],
		// }).compileToV0Message();
		// const tx = new VersionedTransaction(messageV0);
		// const serializedTx = tx.serialize();

		// let serialized_transaction =
		// hex::decode("
		// 010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080010002033f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7f14bf65ad56bd2ba715e45742c231f27d63621cf5b778f37c1a248951d175602b22f4bfe7b663a29da31c40b32ab0b6f96c8ab1946c517b2c056710a352719adb190c20c1c0414dc233cb92bd86b436aeacf0f00b72798464f22e4029ccaa68b010101020927fb829f2e88a4a90400"
		// ).unwrap();

		// BTC Devnet (`6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe`)
		let serialized_transaction =  hex::decode("010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080010002033f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7f14bf65ad56bd2ba715e45742c231f27d63621cf5b778f37c1a248951d175602502b9d5731648a1c61dcf689240e2d2c799393430d9f1d584e368ec4e5243c5f13dcef863a734d75a53ceea4596b64111f9577af432cf6c0c2aed5cb527a733f010101020927fb829f2e88a4a90400").unwrap();
		let encoded_transaction = BASE64_STANDARD.encode(&serialized_transaction);

		println!("encoded_transaction: {:?}", encoded_transaction);

		let simulation_result = sol_rpc_client
			.simulate_transaction(
				encoded_transaction,
				RpcSimulateTransactionConfig {
					sig_verify: false,
					replace_recent_blockhash: true,
					commitment: Some(CommitmentConfig::processed()),
					encoding: Some(UiTransactionEncoding::Base64),
					accounts: None,
					min_context_slot: None,
					inner_instructions: false,
				},
			)
			.await
			.unwrap();
		println!("simulation_result: {:?}", simulation_result);
		println!("return data: {:?}", simulation_result.value.return_data);

		let return_data = simulation_result
			.value
			.return_data
			.as_ref()
			.expect("Expected return data to be Some");

		let decoded_return_data = BASE64_STANDARD.decode(return_data.data.0.clone()).unwrap();
		assert_eq!(return_data.data.1, UiReturnDataEncoding::Base64);

		println!("decoded_return_data: {:?}", decoded_return_data);

		// Verify length (expect 32 bytes)
		assert_eq!(decoded_return_data.len(), 32);

		// // Manually parse the bytes (little-endian). We could also borsh deserialize it.
		// // Reference code: https://github.com/smartcontractkit/chainlink-solana/blob/develop/contracts/programs/store/src/lib.rs#L219
		// // #[derive(AnchorSerialize, AnchorDeserialize)]
		// // pub struct Round {
		// // 	pub round_id: u32,
		// // 	pub slot: u64,
		// // 	pub timestamp: u32,
		// // 	pub answer: i128,
		// // }
		let round_id = u32::from_le_bytes(decoded_return_data[0..4].try_into().unwrap());
		let slot = u64::from_le_bytes(decoded_return_data[4..12].try_into().unwrap());
		let timestamp = u32::from_le_bytes(decoded_return_data[12..16].try_into().unwrap());
		let answer = i128::from_le_bytes(decoded_return_data[16..32].try_into().unwrap());

		println!(
			"Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}",
			round_id, slot, timestamp, answer
		);
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn test_sol_simulate_transaction_decimals() {
		let sol_rpc_client = SolRpcClient::new(
			SecretUrl::from("https://api.devnet.solana.com".to_string()),
			Some(SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap()),
		)
		.unwrap()
		.await;

		// Query decimals (same query just changing the queryByte)
		// BTC Devnet
		// let serialized_transaction =
		// hex::decode("
		// 010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080010002033f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7f14bf65ad56bd2ba715e45742c231f27d63621cf5b778f37c1a248951d175602502b9d5731648a1c61dcf689240e2d2c799393430d9f1d584e368ec4e5243c5f68fc98d59aef5e74ba3e29391f7c1be4701183c145c2fae4cb513d906bf53efb010101020927fb829f2e88a4a90100"
		// ).unwrap(); ETH Devnet
		// let serialized_transaction =
		// hex::decode("
		// 010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080010002033f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7f14bf65ad56bd2ba715e45742c231f27d63621cf5b778f37c1a248951d1756024b9be964820950a986a6318e5c4639a02e3e1bcf24f4767ac622414d6690fd6a1c16a7f6351b28c0d5c39d8483ce2319373696d3ef6ed1cb0e836b6310d99b4e010101020927fb829f2e88a4a90100"
		// ).unwrap(); Manual ETH Devnet => Using the BTC payload but insterting manually the ETH
		// Address => Seems to work => Following bytes that change seem to be part of the recent
		// blockhash, which doesn't matter. Therefore we can just have a hardcoded payload and
		// just insert the address neeeded (decoded bs58) depending on the network.
		let serialized_transaction = hex::decode("010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080010002033f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7f14bf65ad56bd2ba715e45742c231f27d63621cf5b778f37c1a248951d1756024b9be964820950a986a6318e5c4639a02e3e1bcf24f4767ac622414d6690fd6a68fc98d59aef5e74ba3e29391f7c1be4701183c145c2fae4cb513d906bf53efb010101020927fb829f2e88a4a90100").unwrap();

		// Trying manually to insert a different address
		let encoded_transaction = BASE64_STANDARD.encode(&serialized_transaction);

		let simulation_result = sol_rpc_client
			.simulate_transaction(
				encoded_transaction,
				RpcSimulateTransactionConfig {
					sig_verify: false,
					replace_recent_blockhash: true,
					commitment: Some(CommitmentConfig::processed()),
					encoding: Some(UiTransactionEncoding::Base64),
					accounts: None,
					min_context_slot: None,
					inner_instructions: false,
				},
			)
			.await
			.unwrap();

		let return_data = simulation_result
			.value
			.return_data
			.as_ref()
			.expect("Expected return data to be Some");

		let decoded_return_data = BASE64_STANDARD.decode(return_data.data.0.clone()).unwrap();

		assert_eq!(decoded_return_data.len(), 1);
		let value: u8 = decoded_return_data[0];

		// BTC has 8 decimals
		assert_eq!(value, 8);
	}
}
