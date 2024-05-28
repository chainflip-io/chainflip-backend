use futures_core::Future;

use reqwest::{header::CONTENT_TYPE, Client};

use serde_json::{from_value, json};

use tracing::error;
use utilities::make_periodic_tick;

use crate::{btc::rpc::Error, constants::RPC_RETRY_CONNECTION_INTERVAL};
use utilities::redact_endpoint_secret::SecretUrl;

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
				match get_genesis_hash(&client, &endpoint).await {
					Ok(genesis_hash) => match expected_genesis_hash {
						None => {
							warn!("Skipping Solana genesis hash check");
							break;
						},
						Some(expected_hash) if expected_hash == genesis_hash => {
							break;
						},
						Some(_) => {
							error!(
                                        "Connected to Solana node at {0} but the genesis hash {genesis_hash} does not match the expected genesis hash. Please check your CFE configuration file.", endpoint
                                    )
						},
					},
					Err(e) => tracing::error!(
						"Cannot connect to Solana node at {1} with error: {e}. \
                                Please check your CFE configuration file. Retrying in {:?}...",
						poll_interval.period(),
						endpoint
					),
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

	println!("request_body: {:?}", request_body);

	let response = client
		.post(endpoint.as_ref())
		.header(CONTENT_TYPE, "application/json")
		.json(&request_body)
		.send()
		.await
		.map_err(Error::Transport)?;

	let mut json = response.json::<serde_json::Value>().await.map_err(Error::Transport)?;

	if json["error"].is_object() {
		return Err(Error::Rpc(serde_json::from_value(json["error"].clone()).map_err(Error::Json)?));
	}
	println!("json result: {:?}", json["result"]);
	Ok(json["result"].take())
}

/// Get the Solana Network genesis hash by calling the `getGenesisHash` RPC.
async fn get_genesis_hash(client: &Client, endpoint: &SecretUrl) -> anyhow::Result<SolHash> {
	// Call `call_rpc_raw` and get the JSON value
	let json_value = call_rpc_raw(client, endpoint, "getGenesisHash", None)
		.await
		.map_err(anyhow::Error::msg)?;

	// Extract the `result` field from the JSON value
	let genesis_hash_str = json_value
		.as_str()
		.ok_or(anyhow!("Missing or empty `result` field in getGenesisHash response"))?;

	// Parse the genesis hash string into a `SolHash`
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
		search_transaction_history: bool,
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
		search_transaction_history: bool,
	) -> Result<Response<Vec<Option<TransactionStatus>>>> {
		let response = self
			.call_rpc(
				"getSignatureStatuses",
				Some(json!([
					signatures,
					json!({
						"searchTransactionHistory": search_transaction_history
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

	// We expect a fully-signed transaction as encoded string. We might have to encode it here
	// otherwise but the serialization is expected to be done by the caller.
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
}

#[cfg(test)]
mod tests {
	// use utilities::testing::logging::init_test_logger;

	use super::*;

	#[test]
	fn test_encoding() {
		let pubkey = SolAddress::from_str("vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg").unwrap();
		let encoded = encode_pubkey(&pubkey);
		assert_eq!(encoded, "vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg");
	}

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

	#[tokio::test]
	async fn test_sol_devnet() {
		// init_test_logger();

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

		let signature_status =
			sol_rpc_client.get_signature_statuses(&[signature], true).await.unwrap();

		let confirmation_status = signature_status
			.value
			.first()
			.and_then(Option::as_ref)
			.and_then(|ts| ts.confirmation_status.as_ref())
			.expect("Expected confirmation_status to be Some");

		println!("Signature status: {:?}", signature_status);
		assert_eq!(confirmation_status, &TransactionConfirmationStatus::Finalized);
	}

	// TODO: Add test for send transaction
}
