use futures_core::Future;
// use subxt::ext::sp_runtime::print;
use thiserror::Error;

use reqwest::{header::CONTENT_TYPE, Client};
use serde::{Deserialize, Serialize};

use serde;
use serde_bytes;
use serde_json::{from_value, json};

use tracing::error;
use utilities::make_periodic_tick;

use crate::{constants::RPC_RETRY_CONNECTION_INTERVAL, settings::HttpBasicAuthEndpoint};

use anyhow::{anyhow, Context, Result};
use tracing::warn;

use cf_chains::sol::{sol_tx_core::Pubkey, SolHash};

use super::{commitment_config::CommitmentConfig, rpc_client_api::*};
use std::str::FromStr;

// From jsonrpc crate
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcError {
	/// The integer identifier of the error
	pub code: i32,
	/// A string describing the error
	pub message: String,
	/// Additional data specific to the error
	pub data: Option<Box<serde_json::value::RawValue>>,
}

#[derive(Error, Debug)]
pub enum Error {
	Transport(reqwest::Error),
	Json(serde_json::Error),
	Rpc(RpcError),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match *self {
			Error::Transport(ref e) => write!(f, "Transport error: {}", e),
			Error::Json(ref e) => write!(f, "JSON decode error: {}", e),
			Error::Rpc(ref e) => write!(f, "RPC error response: {:?}", e),
		}
	}
}

#[derive(Clone)]
pub struct SolRpcClient {
	// Internally the Client is Arc'd
	client: Client,
	endpoint: HttpBasicAuthEndpoint,
}

impl SolRpcClient {
	pub fn new(
		// TODO: Should we use SecretUrl instead?
		endpoint: HttpBasicAuthEndpoint,
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
                                        "Connected to Solana node at {0} but the genesis hash {genesis_hash} does not match the expected genesis hash. Please check your CFE configuration file.", endpoint.http_endpoint
                                    )
						},
					},
					Err(e) => tracing::error!(
						"Cannot connect to Solana node at {1} with error: {e}. \
                                Please check your CFE configuration file. Retrying in {:?}...",
						poll_interval.period(),
						endpoint.http_endpoint
					),
				}
			}
			Self { client, endpoint }
		})
	}

	// async fn call_rpc<T: for<'a> serde::de::Deserialize<'a>>(
	async fn call_rpc(&self, method: &str, params: ReqParams) -> Result<serde_json::Value, Error> {
		call_rpc_raw(&self.client, &self.endpoint, method, params).await
	}
}

#[derive(Clone, Debug)]
enum ReqParams {
	Empty,
	Single(serde_json::Value),
}

async fn call_rpc_raw(
	client: &Client,
	endpoint: &HttpBasicAuthEndpoint,
	method: &str,
	params: ReqParams,
) -> Result<serde_json::Value, Error> {
	let request_body = match params.clone() {
		ReqParams::Empty => vec![json!({
			"jsonrpc": "2.0",
			"id": 0,
			"method": method,
			"params": []
		})],
		ReqParams::Single(params) => vec![json!({
			"jsonrpc": "2.0",
			"id": 0,
			"method": method,
			"params": params
		})],
	};

	println!("request_body: {:?}", request_body);

	let response = client
		.post(endpoint.http_endpoint.as_ref())
		.basic_auth(&endpoint.basic_auth_user, Some(&endpoint.basic_auth_password))
		.header(CONTENT_TYPE, "application/json")
		.json(&request_body)
		.send()
		.await
		.map_err(Error::Transport)?;

	println!("response: {:?}", response);

	let mut json = response.json::<serde_json::Value>().await.map_err(Error::Transport)?;

	println!("json: {:?}", json);

	// TODO: This is a bit hacky and assumes it will be an array of length 1.
	if let Some(array) = json.as_array() {
		if let Some(first_element) = array.get(0) {
			println!("result: {:?}", first_element["result"].clone());
			Ok(first_element["result"].clone())
		} else {
			println!("Array is empty");
			Err(Error::Rpc(RpcError {
				code: -32603,
				message: "Internal error".to_string(),
				data: None,
			}))
		}
	} else {
		println!("Not an array");
		Err(Error::Rpc(RpcError {
			code: -32603,
			message: "Internal error".to_string(),
			data: None,
		}))
	}
}

/// Get the Solana Network genesis hash by calling the `getGenesisHash` RPC.
async fn get_genesis_hash(
	client: &Client,
	endpoint: &HttpBasicAuthEndpoint,
) -> anyhow::Result<SolHash> {
	// Call `call_rpc_raw` and get the JSON value
	let json_value = call_rpc_raw(client, endpoint, "getGenesisHash", ReqParams::Empty)
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

#[async_trait::async_trait]
pub trait SolRpcApi {
	// TODO: Change naming to get_block and make config an option
	async fn get_block_with_config(
		&self,
		slot: u64,
		config: RpcBlockConfig,
	) -> anyhow::Result<UiConfirmedBlock>;
	async fn get_recent_prioritization_fees(&self) -> anyhow::Result<Vec<RpcPrioritizationFee>>;
	async fn get_multiple_accounts_with_config(
		&self,
		// Using Strings for now as we can't convert Pubkey to String at the moment
		pubkeys: &[String],
		config: RpcAccountInfoConfig,
	) -> Result<Response<Vec<Option<UiAccount>>>>;
}

#[async_trait::async_trait]
impl SolRpcApi for SolRpcClient {
	async fn get_block_with_config(
		&self,
		slot: u64,
		config: RpcBlockConfig,
	) -> anyhow::Result<UiConfirmedBlock> {
		// TODO: Should we harcode to not get transactions nor rewards?
		let response = self
			.call_rpc("getBlock", ReqParams::Single(json!([slot, json!(config)])))
			.await?;
		let block: UiConfirmedBlock =
			from_value(response).map_err(|err| anyhow!("Failed to parse block {}", err))?;
		Ok(block)
	}

	async fn get_recent_prioritization_fees(&self) -> anyhow::Result<Vec<RpcPrioritizationFee>> {
		let response = self.call_rpc("getRecentPrioritizationFees", ReqParams::Empty).await?;
		let fees: Vec<RpcPrioritizationFee> = from_value(response)
			.map_err(|err| anyhow!("Failed to parse prioritization fees: {}", err))?;
		Ok(fees)
	}

	async fn get_multiple_accounts_with_config(
		&self,
		pubkeys: &[String],
		config: RpcAccountInfoConfig,
	) -> Result<Response<Vec<Option<UiAccount>>>> {
		// TODO: We might want to request a data slice => No data at all for Sol, we only need
		// lamports, and only balance data for tokens.

		// Convert pubkeys to a vector of strings
		let pubkeys: Vec<_> = pubkeys.iter().map(|pubkey| pubkey).collect();

		let response = self
			.call_rpc("getMultipleAccounts", ReqParams::Single(json!([pubkeys, json!(config)])))
			.await?;
		println!("response: {:?}", response);

		// TODO: Could we put this in call_rpc with a passed generic type?
		let Response { context, value: accounts } =
			serde_json::from_value::<Response<Vec<Option<UiAccount>>>>(response.clone())?;
		Ok(Response { context, value: accounts })
	}
}

#[cfg(test)]
mod tests {
	// use utilities::testing::logging::init_test_logger;

	use super::*;

	#[tokio::test]
	async fn test_sol_asyc() {
		let sol_rpc_client = SolRpcClient::new(
			HttpBasicAuthEndpoint {
				http_endpoint: "https://api.testnet.solana.com".into(),
				basic_auth_user: "flip".to_string(),
				basic_auth_password: "flip".to_string(),
			},
			None,
		)
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
			HttpBasicAuthEndpoint {
				http_endpoint: "https://api.devnet.solana.com".into(),
				basic_auth_user: "flip".to_string(),
				basic_auth_password: "flip".to_string(),
			},
			Some(SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap()),
		)
		.unwrap()
		.await;

		let priority_fees = sol_rpc_client.get_recent_prioritization_fees().await.unwrap();
		println!("priority_fees: {:?}", priority_fees);

		let result = sol_rpc_client
			.get_multiple_accounts_with_config(
				&["vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg".to_string()],
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

		let result = sol_rpc_client
			.get_multiple_accounts_with_config(
				&[
					"vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg".to_string(),
					"4fYNw3dojWmQ4dXtSGE9epjRGy9pFSx62YypT7avPYvA".to_string(),
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
			.get_block_with_config(
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
}
