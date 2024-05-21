use cf_chains::btc::BitcoinNetwork;
use futures_core::Future;
use subxt::ext::sp_runtime::print;
use thiserror::Error;

use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};

use serde;
use serde_json::{json, Map};

use tracing::error;
use utilities::make_periodic_tick;

use crate::{constants::RPC_RETRY_CONNECTION_INTERVAL, settings::HttpBasicAuthEndpoint};

use anyhow::{anyhow, Context, Result};
use tracing::warn;

use cf_chains::sol::SolHash;
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
		// TODO: We should probably use SecretUrl isntead, as in the http_sender.rs
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

	async fn call_rpc<T: for<'a> serde::de::Deserialize<'a>>(
		&self,
		method: &str,
		params: ReqParams,
	) -> Result<Vec<T>> {
		call_rpc_raw(&self.client, &self.endpoint, method, params)
			.await?
			.into_iter()
			.map(|v| T::deserialize(v).map_err(anyhow::Error::msg))
			.collect::<Result<_>>()
	}
}

#[derive(Clone, Debug)]
enum ReqParams {
	Empty,
	Batch(Vec<serde_json::Value>),
}

async fn call_rpc_raw(
	client: &Client,
	endpoint: &HttpBasicAuthEndpoint,
	method: &str,
	params: ReqParams,
) -> Result<Vec<serde_json::Value>, Error> {
	let request_body = match params.clone() {
		ReqParams::Empty => vec![json!({
			"jsonrpc": "2.0",
			"id": 0,
			"method": method,
			"params": []
		})],
		ReqParams::Batch(params) => params
			.into_iter()
			.enumerate()
			.map(|(i, p)| {
				json!({
					"jsonrpc": "2.0",
					"id": i,
					"method": method,
					"params": p
				})
			})
			.collect::<Vec<serde_json::Value>>(),
	};

	let response = client
		.post(endpoint.http_endpoint.as_ref())
		.basic_auth(&endpoint.basic_auth_user, Some(&endpoint.basic_auth_password))
		.json(&request_body)
		.send()
		.await
		.map_err(Error::Transport)?;

	let response = response
		.json::<Vec<serde_json::Value>>()
		.await
		.map_err(Error::Transport)
		.and_then(|result| match params {
			ReqParams::Batch(params) =>
				if params.len() == result.len() {
					// a bunch of json result values containing an error and a result field.
					Ok(result)
				} else {
					Err(Error::Rpc(RpcError {
						code: -1,
						message: "Incorrect response number for batch request".to_string(),
						data: None,
					}))
				},
			ReqParams::Empty => Ok(result),
		})?;

	response
		.into_iter()
		.map(|r| {
			let error = &r["error"];
			if !error.is_null() {
				Err(Error::Rpc(serde_json::from_value(error.clone()).map_err(Error::Json)?))
			} else {
				Ok(r["result"].to_owned())
			}
		})
		.collect::<Result<_, Error>>()
}

/// Get the Solana Network genesis hash by calling the `getGenesisHash` RPC.
async fn get_genesis_hash(
	client: &Client,
	endpoint: &HttpBasicAuthEndpoint,
) -> anyhow::Result<SolHash> {
	// Using `call_rpc_raw` so we don't have to deserialize the whole response.
	let json_value = call_rpc_raw(client, endpoint, "getGenesisHash", ReqParams::Empty)
		.await
		.map_err(anyhow::Error::msg)?
		.into_iter()
		.next()
		.ok_or(anyhow!("Missing response from getGenesisHash"))?;

	println!("json_value: {:?}", json_value.as_str().unwrap());
	let genesis_hash_str = json_value
		.as_str()
		.ok_or(anyhow!("Missing or empty `result` field in getGenesisHash response"))?;
	println!("genesis_hash_str: {:?}", genesis_hash_str);

	// TODO: This conversion is incorrect but the values look correct
	Ok(SolHash::from_str(genesis_hash_str).context("Invalid genesis hash")?)
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
	// pub hash: bitcoin::BlockHash,
	pub confirmations: i32,
	// pub height: u64,
	// pub version: Version,
	// pub version_hex: Option<String>,
	// // Don't care to write custom deserializer for this
	// #[serde(rename = "merkleroot")]
	// pub merkle_root: bitcoin::hash_types::TxMerkleNode,
	// pub time: usize,
	// #[serde(rename = "mediantime")]
	// pub median_time: Option<usize>,
	// pub nonce: u32,
	// pub bits: String,
	// pub difficulty: Difficulty,
	// // Don't care to write custom deserializer for this
	// pub chainwork: Option<String>,
	// pub n_tx: usize,
	// #[serde(rename = "previousblockhash")]
	// pub previous_block_hash: Option<bitcoin::BlockHash>,
	// #[serde(rename = "nextblockhash")]
	// pub next_block_hash: Option<bitcoin::BlockHash>,

	// pub strippedsize: Option<usize>,
	// pub size: Option<usize>,
	// pub weight: Option<usize>,
}

#[async_trait::async_trait]
pub trait SolRpcApi {
	// async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader>;
}

#[async_trait::async_trait]
impl SolRpcApi for SolRpcClient {
	// async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
	// 	Ok(self
	// 		.call_rpc("getblockheader", ReqParams::Batch(vec![json!([json!(block_hash)])]))
	// 		.await?
	// 		.into_iter()
	// 		.next()
	// 		.ok_or_else(|| anyhow!("Response missing block header"))?)
	// }
}

#[cfg(test)]
mod tests {

	use super::*;

	#[tokio::test]
	async fn test_sol_asyc() {
		// 		init_test_logger();

		let client = SolRpcClient::new(
			HttpBasicAuthEndpoint {
				http_endpoint: "https://api.devnet.solana.com".into(),
				basic_auth_user: "flip".to_string(),
				basic_auth_password: "flip".to_string(),
			},
			None,
		)
		.unwrap()
		.await;

		let genesis_hash = get_genesis_hash(&client.client, &client.endpoint).await.unwrap();
		println!("genesis_hash: {:?}", genesis_hash);

		// 		let result: Result<VerboseBlock, _> = serde_path_to_error::deserialize(jd);

		// 		match result {
		// 			Ok(vb) => {
		// 				println!("vb: {vb:?}");
		// 				println!("Verbose block fee: {}", vb.txdata[1].fee.unwrap());
		// 			},
		// 			Err(e) => panic!("error: {e:?}"),
		// 		}

		// 		let block_hash_zero = client.block_hash(0).await.unwrap();

		// 		println!("block_hash_zero: {block_hash_zero:?}");

		// 		let block_zero = client.block(block_hash_zero).await.unwrap();

		// 		println!("block_zero: {block_zero:?}");

		// 		let next_block_fee_rate = client.next_block_fee_rate().await.unwrap();

		// 		println!("next_block_fee_rate: {next_block_fee_rate:?}");

		// 		let best_block_hash = client.best_block_hash().await.unwrap();

		// 		println!("best_block_hash: {best_block_hash:?}");

		// 		let block_header = client.block_header(best_block_hash).await.unwrap();

		// 		println!("block_header: {block_header:?}");

		// 		let v_block = client.block(best_block_hash).await.unwrap();

		// 		println!("verbose block: {v_block:?}");

		// 		println!("number of txs: {}", v_block.txdata.len());

		// 		let tx = &v_block.txdata[0];

		// 		let raw_transaction =
		// client.get_raw_transactions(vec![tx.txid]).await.unwrap()[0].txid(); 		assert_eq!
		// (raw_transaction, tx.txid);

		// 		// let average_block_fee_rate =
		// 		// client.average_block_fee_rate(best_block_hash).await.unwrap();

		// 		// println!("average_block_fee_rate: {average_block_fee_rate}");

		// 		// Generate new hex bytes using ./bouncer/commands/create_raw_btc_tx.ts;
		// 		// let hex_str =
		// 		// "0200000000010133e287d3a464b226a1917303e1714af508a6bfe219265184a93c7f78851085a30000000000fdffffff0200e1f505000000001976a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac58d94a1f000000001600145baa7941ea1268fbd6279a0408a0419f8acd8245024730440220590dcc64661a362b54543f66d3cd24fdeae8a210643ad4f6fd39031281a9657902201f7a750d01f7cfc948ae4f8b76221b5c325f8ff82ac1b0d1d9a927632b40dd6001210386dc234ecbc4e677b927da260349cbd399c622507feb9dd2895a3537f6d4aa5d00000000"
		// ;

		// 		// let bytes = hex::decode(hex_str).unwrap();

		// 		// let send_raw_transaction = client.send_raw_transaction(bytes).await.unwrap();

		// 		// println!("tx_id: {:?}", send_raw_transaction);

		// 		// let best_
	}
}
