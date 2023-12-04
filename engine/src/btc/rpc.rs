use cf_chains::btc::BitcoinNetwork;
use futures_core::Future;
use thiserror::Error;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use serde;
use serde_json::json;

use bitcoin::{block::Version, Amount, Block, BlockHash, Transaction, Txid};
use tracing::error;
use utilities::make_periodic_tick;

use crate::{constants::RPC_RETRY_CONNECTION_INTERVAL, settings::HttpBasicAuthEndpoint};

use anyhow::{anyhow, Context, Result};

// https://github.com/bitcoin/bitcoin/blob/fb7b5293844ea6adc5dcf5ad0a0c5890b4495939/src/rpc/protocol.h#L58
const RPC_VERIFY_ALREADY_IN_CHAIN: i32 = -27;

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

#[derive(Clone, Debug, Deserialize)]
struct FeeRateResponse {
	#[serde(
		default,
		rename = "feerate",
		skip_serializing_if = "Option::is_none",
		with = "bitcoin::amount::serde::as_btc::opt"
	)]
	feerate: Option<Amount>,

	// We need it for the deserialization, but we don't use it.
	#[allow(dead_code)]
	blocks: u32,
}

#[derive(Clone)]
pub struct BtcRpcClient {
	// Internally the Client is Arc'd
	client: Client,
	endpoint: HttpBasicAuthEndpoint,
}

impl BtcRpcClient {
	pub fn new(
		endpoint: HttpBasicAuthEndpoint,
		expected_btc_network: Option<BitcoinNetwork>,
	) -> Result<impl Future<Output = Self>> {
		let client = Client::builder().build()?;

		Ok(async move {
			if let Some(expected_btc_network) = expected_btc_network {
				let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
				loop {
					poll_interval.tick().await;
					match get_bitcoin_network(&client, &endpoint).await {
						Ok(network) if network == expected_btc_network => break,
						Ok(network) => {
							error!(
									"Connected to Bitcoin node but with incorrect network name `{network}`, expected `{expected_btc_network}` on endpoint {}. Please check your CFE
									configuration file...",
									endpoint.http_endpoint
								);
						},
						Err(e) => error!(
							"Failure connecting to Bitcoin node at {} with error: {e}. Please check your CFE
								configuration file. Retrying in {:?}...",
							endpoint.http_endpoint,
							poll_interval.period()
						),
					}
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
			"jsonrpc": "1.0",
			"id": 0,
			"method": method,
			"params": []
		})],
		ReqParams::Batch(params) => params
			.into_iter()
			.enumerate()
			.map(|(i, p)| {
				json!({
					"jsonrpc": "1.0",
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

/// Get the BitcoinNetwork by calling the `getblockchaininfo` RPC.
async fn get_bitcoin_network(
	client: &Client,
	endpoint: &HttpBasicAuthEndpoint,
) -> anyhow::Result<BitcoinNetwork> {
	// Using `call_rpc_raw` so we don't have to deserialize the whole response.
	let json_value = call_rpc_raw(client, endpoint, "getblockchaininfo", ReqParams::Empty)
		.await
		.map_err(anyhow::Error::msg)?
		.into_iter()
		.next()
		.ok_or(anyhow!("Missing response from getblockchaininfo"))?;
	let network_name = json_value["chain"]
		.as_str()
		.ok_or(anyhow!("Missing or empty `chain` field in getblockchaininfo response"))?;

	BitcoinNetwork::try_from(network_name)
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
	pub hash: bitcoin::BlockHash,
	pub confirmations: i32,
	pub height: u64,
	pub version: Version,
	// Don't care to write custom deserializer for this
	#[serde(skip)]
	pub version_hex: Option<Vec<u8>>,
	#[serde(rename = "merkleroot")]
	pub merkle_root: bitcoin::hash_types::TxMerkleNode,
	pub time: usize,
	#[serde(rename = "mediantime")]
	pub median_time: Option<usize>,
	pub nonce: u32,
	pub bits: String,
	pub difficulty: f64,
	// Don't care to write custom deserializer for this
	#[serde(skip)]
	pub chainwork: Vec<u8>,
	pub n_tx: usize,
	#[serde(rename = "previousblockhash")]
	pub previous_block_hash: Option<bitcoin::BlockHash>,
	#[serde(rename = "nextblockhash")]
	pub next_block_hash: Option<bitcoin::BlockHash>,
}

#[async_trait::async_trait]
pub trait BtcRpcApi {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<Block>;

	async fn block_hash(
		&self,
		block_number: cf_chains::btc::BlockNumber,
	) -> anyhow::Result<BlockHash>;

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<cf_chains::btc::BtcAmount>>;

	async fn average_block_fee_rate(
		&self,
		block_hash: BlockHash,
	) -> anyhow::Result<cf_chains::btc::BtcAmount>;

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash>;

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader>;

	async fn get_raw_mempool(&self) -> anyhow::Result<Vec<Txid>>;

	async fn get_raw_transactions(&self, tx_hashes: Vec<Txid>) -> anyhow::Result<Vec<Transaction>>;
}

#[async_trait::async_trait]
impl BtcRpcApi for BtcRpcClient {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<bitcoin::Block> {
		// The 0 arg means we get the response as a hex string, which we use in the custom
		// deserialization.
		let hex_block: Vec<String> = self
			.call_rpc("getblock", ReqParams::Batch(vec![json!([json!(block_hash), json!(0)])]))
			.await?;
		let hex_block = hex_block
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing hex block"))?;
		let hex_bytes = hex::decode(hex_block).context("Response not valid hex")?;
		Ok(bitcoin::consensus::encode::deserialize(&hex_bytes)?)
	}

	async fn block_hash(
		&self,
		block_number: cf_chains::btc::BlockNumber,
	) -> anyhow::Result<BlockHash> {
		Ok(self
			.call_rpc("getblockhash", ReqParams::Batch(vec![json!([json!(block_number)])]))
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing block hash"))?)
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		let tx: Transaction = bitcoin::consensus::encode::deserialize(&transaction_bytes)
			.map_err(|_| anyhow!("Failed to deserialize transaction"))?;
		let derived_txid = tx.txid();

		match call_rpc_raw(
			&self.client,
			&self.endpoint,
			"sendrawtransaction",
			ReqParams::Batch(vec![json!([json!(hex::encode(transaction_bytes))])]),
		)
		.await
		{
			Ok(txids) => {
				let txid = txids
					.into_iter()
					.map(|txid| {
						Txid::deserialize(txid)
							.map_err(|e| anyhow!("Error deserializing response: {e:?}"))
					})
					.next()
					.ok_or_else(|| anyhow!("Response missing txid"))??;
				assert_eq!(txid, derived_txid);
				Ok(txid)
			},
			Err(Error::Rpc(e)) if e.code == RPC_VERIFY_ALREADY_IN_CHAIN => {
				tracing::info!("Transaction already on chain with txid: {:?}", derived_txid);
				Ok(derived_txid)
			},
			Err(e) => Err(anyhow!("Error sending transaction: {:?}", e)),
		}
	}

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<cf_chains::btc::BtcAmount>> {
		let fee_rate_response = self
			.call_rpc::<FeeRateResponse>(
				"estimatesmartfee",
				ReqParams::Batch(vec![json!([json!(1), json!("CONSERVATIVE")])]),
			)
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing fee rate"))?;

		Ok(fee_rate_response.feerate.map(|f| f.to_sat()))
	}

	async fn average_block_fee_rate(
		&self,
		block_hash: BlockHash,
	) -> anyhow::Result<cf_chains::btc::BtcAmount> {
		// https://developer.bitcoin.org/reference/rpc/getblockstats.html
		#[derive(Deserialize, Serialize)]
		pub struct BlockStats {
			#[serde(with = "bitcoin::amount::serde::as_sat")]
			pub avgfeerate: bitcoin::Amount,
		}

		let block_stats: BlockStats = self
			.call_rpc(
				"getblockstats",
				ReqParams::Batch(vec![json!([json!(block_hash), json!(["avgfeerate"])])]),
			)
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing block stats"))?;

		Ok(block_stats.avgfeerate.to_sat().saturating_mul(1024))
	}

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
		Ok(self
			.call_rpc("getbestblockhash", ReqParams::Empty)
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing best block hash"))?)
	}

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
		Ok(self
			.call_rpc("getblockheader", ReqParams::Batch(vec![json!([json!(block_hash)])]))
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing block header"))?)
	}

	async fn get_raw_mempool(&self) -> anyhow::Result<Vec<Txid>> {
		Ok(self
			.call_rpc("getrawmempool", ReqParams::Empty)
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing raw mempool"))?)
	}

	async fn get_raw_transactions(&self, tx_hashes: Vec<Txid>) -> anyhow::Result<Vec<Transaction>> {
		let params = tx_hashes
			.iter()
			.map(|tx_hash| json!([json!(tx_hash), json!(false)]))
			.collect::<Vec<serde_json::Value>>();

		let hex_txs: Vec<String> =
			self.call_rpc("getrawtransaction", ReqParams::Batch(params)).await?;

		hex_txs
			.into_iter()
			.map(|hex| hex::decode(hex).context("Response not valid hex"))
			.collect::<Result<Vec<Vec<u8>>>>()?
			.into_iter()
			.map(|bytes| {
				bitcoin::consensus::encode::deserialize(&bytes)
					.map_err(|_| anyhow!("Failed to deserialize transaction"))
			})
			.collect::<Result<_>>()
	}
}

#[cfg(test)]
mod tests {

	use utilities::testing::logging::init_test_logger;

	use super::*;

	#[tokio::test]
	#[ignore = "requires local node, useful for manual testing"]
	async fn test_btc_async() {
		init_test_logger();

		let client = BtcRpcClient::new(
			HttpBasicAuthEndpoint {
				http_endpoint: "http://localhost:8332".into(),
				basic_auth_user: "flip".to_string(),
				basic_auth_password: "flip".to_string(),
			},
			Some(BitcoinNetwork::Regtest),
		)
		.unwrap()
		.await;

		let block_hash_zero = client.block_hash(0).await.unwrap();

		println!("block_hash_zero: {block_hash_zero:?}");

		let block_zero = client.block(block_hash_zero).await.unwrap();

		println!("block_zero: {block_zero:?}");

		let next_block_fee_rate = client.next_block_fee_rate().await.unwrap();

		println!("next_block_fee_rate: {next_block_fee_rate:?}");

		let best_block_hash = client.best_block_hash().await.unwrap();

		println!("best_block_hash: {best_block_hash:?}");

		let block_header = client.block_header(best_block_hash).await.unwrap();

		println!("block_header: {block_header:?}");

		let average_block_fee_rate = client.average_block_fee_rate(best_block_hash).await.unwrap();

		println!("average_block_fee_rate: {average_block_fee_rate}");

		// Generate new hex bytes using ./bouncer/commands/create_raw_btc_tx.ts;
		// let hex_str =
		// "0200000000010133e287d3a464b226a1917303e1714af508a6bfe219265184a93c7f78851085a30000000000fdffffff0200e1f505000000001976a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac58d94a1f000000001600145baa7941ea1268fbd6279a0408a0419f8acd8245024730440220590dcc64661a362b54543f66d3cd24fdeae8a210643ad4f6fd39031281a9657902201f7a750d01f7cfc948ae4f8b76221b5c325f8ff82ac1b0d1d9a927632b40dd6001210386dc234ecbc4e677b927da260349cbd399c622507feb9dd2895a3537f6d4aa5d00000000";

		// let bytes = hex::decode(hex_str).unwrap();

		// let send_raw_transaction = client.send_raw_transaction(bytes).await.unwrap();

		// println!("tx_id: {:?}", send_raw_transaction);

		// let best_
	}
}
