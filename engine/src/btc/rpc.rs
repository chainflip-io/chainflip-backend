use thiserror::Error;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use serde;
use serde_json::json;

use bitcoin::{block::Version, Amount, Block, BlockHash, Txid};

use crate::settings;

use anyhow::{Context, Result};

#[cfg(test)]
use mockall::automock;

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
	// internally the Client is Arc'd
	client: Client,
	url: String,
	user: String,
	password: String,
}

impl BtcRpcClient {
	pub fn new(btc_settings: settings::Btc) -> Result<Self> {
		Ok(Self {
			client: Client::builder().build()?,
			url: btc_settings.http_node_endpoint.clone(),
			user: btc_settings.rpc_user.clone(),
			password: btc_settings.rpc_password.clone(),
		})
	}

	async fn call_rpc<T: for<'a> serde::de::Deserialize<'a>>(
		&self,
		method: &str,
		params: Vec<serde_json::Value>,
	) -> Result<T, Error> {
		let request_body = json!({
			"jsonrpc": "1.0",
			"id":"1",
			"method": method,
			"params": params
		});

		let response = &self
			.client
			.post(&self.url)
			.basic_auth(&self.user, Some(&self.password))
			.json(&request_body)
			.send()
			.await
			.map_err(Error::Transport)?
			.json::<serde_json::Value>()
			.await
			.map_err(Error::Transport)?;

		let error = &response["error"];
		if !error.is_null() {
			Err(Error::Rpc(serde_json::from_value(error.clone()).map_err(Error::Json)?))
		} else {
			Ok(T::deserialize(&response["result"]).map_err(Error::Json))?
		}
	}
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
#[cfg_attr(test, automock)]
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
}

#[async_trait::async_trait]
impl BtcRpcApi for BtcRpcClient {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<bitcoin::Block> {
		// The 0 arg means we get the response as a hex string, which we use in the custom
		// deserialization.
		let hex_block: String =
			self.call_rpc("getblock", vec![json!(block_hash), json!(0)]).await?;
		let hex_bytes = hex::decode(hex_block).context("Response not valid hex")?;
		Ok(bitcoin::consensus::encode::deserialize(&hex_bytes)?)
	}

	async fn block_hash(
		&self,
		block_number: cf_chains::btc::BlockNumber,
	) -> anyhow::Result<BlockHash> {
		Ok(self.call_rpc("getblockhash", vec![json!(block_number)]).await?)
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
		Ok(self
			.call_rpc("sendrawtransaction", vec![json!(hex::encode(transaction_bytes))])
			.await?)
	}

	async fn next_block_fee_rate(&self) -> anyhow::Result<Option<cf_chains::btc::BtcAmount>> {
		let fee_rate_response: FeeRateResponse =
			self.call_rpc("estimatesmartfee", vec![json!(1), json!("CONSERVATIVE")]).await?;
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
			.call_rpc("getblockstats", vec![json!(block_hash), json!(["avgfeerate"])])
			.await?;

		Ok(block_stats.avgfeerate.to_sat().saturating_mul(1024))
	}

	async fn best_block_hash(&self) -> anyhow::Result<BlockHash> {
		Ok(self.call_rpc("getbestblockhash", vec![]).await?)
	}

	async fn block_header(&self, block_hash: BlockHash) -> anyhow::Result<BlockHeader> {
		Ok(self.call_rpc("getblockheader", vec![json!(block_hash)]).await?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	#[ignore = "requires local node, useful for manual testing"]
	async fn test_btc_async() {
		let client = BtcRpcClient::new(&settings::Btc {
			http_node_endpoint: "http://localhost:8332".to_string(),
			rpc_user: "flip".to_string(),
			rpc_password: "flip".to_string(),
		})
		.unwrap();

		let block_hash_zero = client.block_hash(0).await.unwrap();

		println!("block_hash_zero: {:?}", block_hash_zero);

		let block_zero = client.block(block_hash_zero).await.unwrap();

		println!("block_zero: {:?}", block_zero);

		let next_block_fee_rate = client.next_block_fee_rate().await.unwrap();

		println!("next_block_fee_rate: {:?}", next_block_fee_rate);

		let average_block_fee_rate = client.average_block_fee_rate(block_hash_zero).await.unwrap();

		println!("average_block_fee_rate: {}", average_block_fee_rate);

		let best_block_hash = client.best_block_hash().await.unwrap();

		println!("best_block_hash: {:?}", best_block_hash);

		let block_header = client.block_header(best_block_hash).await.unwrap();

		println!("block_header: {:?}", block_header);

		// Generate new hex bytes using ./bouncer/commands/create_raw_btc_tx.ts;
		// let hex_str =
		// "0200000000010133e287d3a464b226a1917303e1714af508a6bfe219265184a93c7f78851085a30000000000fdffffff0200e1f505000000001976a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac58d94a1f000000001600145baa7941ea1268fbd6279a0408a0419f8acd8245024730440220590dcc64661a362b54543f66d3cd24fdeae8a210643ad4f6fd39031281a9657902201f7a750d01f7cfc948ae4f8b76221b5c325f8ff82ac1b0d1d9a927632b40dd6001210386dc234ecbc4e677b927da260349cbd399c622507feb9dd2895a3537f6d4aa5d00000000";

		// let bytes = hex::decode(hex_str).unwrap();

		// let send_raw_transaction = client.send_raw_transaction(bytes).await.unwrap();

		// println!("tx_id: {:?}", send_raw_transaction);

		// let best_
	}
}
