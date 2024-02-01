use cf_chains::btc::BitcoinNetwork;
use futures_core::Future;
use thiserror::Error;

use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};

use serde;
use serde_json::{json, Map};

use bitcoin::{
	absolute, block::Version, Amount, BlockHash, ScriptBuf, Sequence, Transaction, Txid,
};
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
							error!("Connected to Bitcoin node but with incorrect network name `{network}`, expected `{expected_btc_network}` on endpoint {}. \
							Please check your CFE configuration file...", endpoint.http_endpoint);
						},
						Err(e) => error!(
							"Failure connecting to Bitcoin node at {} with error: {e}. \
							Please check your CFE configuration file. Retrying in {:?}...",
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

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum VerboseOutPoint {
	#[serde(rename = "coinbase")]
	Coinbase { coinbase: String },
	#[serde(rename = "txid")]
	Txid { txid: Txid, vout: u32 },
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct VerboseTxIn {
	#[serde(flatten)]
	pub outpoint: VerboseOutPoint,
	pub txinwitness: Option<Vec<String>>,
	pub sequence: Sequence,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct VerboseTxOut {
	#[serde(with = "bitcoin::amount::serde::as_btc")]
	pub value: Amount,
	pub n: u64,
	#[serde(rename = "scriptPubKey")]
	#[serde(deserialize_with = "deserialize_scriptpubkey")]
	pub script_pubkey: ScriptBuf,
}

fn deserialize_scriptpubkey<'de, D>(deserializer: D) -> Result<ScriptBuf, D::Error>
where
	D: Deserializer<'de>,
{
	#[derive(Deserialize)]
	struct Helper {
		hex: String,
	}
	let helper = Helper::deserialize(deserializer)?;
	Ok(ScriptBuf::from(hex::decode(helper.hex).map_err(serde::de::Error::custom)?))
}

// This is a work around for this bug, it is effectively a catch all.
// https://github.com/serde-rs/json/issues/721
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Difficulty {
	Number(f64),
	Object(Map<String, serde_json::Value>),
}

/// Transaction type when the verbose flag is used.
/// We don't currently include the transaction version here, because according
/// to the specifications it should be a signed 32bit integer, but on BTC testnet,
/// block 000000002f4830471b6b346578546615c031b99da5e7fabeac119b63f1843f82 contains
/// a transaction with a version of 4294967295 which cannot be parsed into i32.
/// Since we don't currently use this value, it is removed for now.
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerboseTransaction {
	pub txid: Txid,
	pub hash: Txid,
	pub size: usize,
	pub vsize: usize,
	pub weight: usize,
	pub locktime: absolute::LockTime,
	pub vin: Vec<VerboseTxIn>,
	pub vout: Vec<VerboseTxOut>,
	#[serde(
		default,
		skip_serializing_if = "Option::is_none",
		with = "bitcoin::amount::serde::as_btc::opt"
	)]
	pub fee: Option<Amount>,
	pub hex: String,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct VerboseBlock {
	/// The block header
	#[serde(flatten)]
	pub header: BlockHeader,

	/// List of transactions contained in the block\
	#[serde(rename = "tx")]
	pub txdata: Vec<VerboseTransaction>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
	pub hash: bitcoin::BlockHash,
	pub confirmations: i32,
	pub height: u64,
	pub version: Version,
	pub version_hex: Option<String>,
	// Don't care to write custom deserializer for this
	#[serde(rename = "merkleroot")]
	pub merkle_root: bitcoin::hash_types::TxMerkleNode,
	pub time: usize,
	#[serde(rename = "mediantime")]
	pub median_time: Option<usize>,
	pub nonce: u32,
	pub bits: String,
	pub difficulty: Difficulty,
	// Don't care to write custom deserializer for this
	pub chainwork: Option<String>,
	pub n_tx: usize,
	#[serde(rename = "previousblockhash")]
	pub previous_block_hash: Option<bitcoin::BlockHash>,
	#[serde(rename = "nextblockhash")]
	pub next_block_hash: Option<bitcoin::BlockHash>,

	pub strippedsize: Option<usize>,
	pub size: Option<usize>,
	pub weight: Option<usize>,
}

#[async_trait::async_trait]
pub trait BtcRpcApi {
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock>;

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
	async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
		Ok(self
			.call_rpc("getblock", ReqParams::Batch(vec![json!([json!(block_hash), json!(2)])]))
			.await?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow!("Response missing block"))?)
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
	async fn test_nonstandard_version_tx() {
		// This is block 000000002f4830471b6b346578546615c031b99da5e7fabeac119b63f1843f82 from the
		// BTC testnet, which contains a transaction with version 4294967295 (violating specs that
		// say transaction versions should be of type i32)
		let block_data: &str = r#"{"hash":"000000002f4830471b6b346578546615c031b99da5e7fabeac119b63f1843f82","confirmations":19,"height":2575399,"version":536870914,"versionHex":"20000002","merkleroot":"be9df28c845cc1b8decaf2deed6772635321545923fd574cc58f2d4236bb6cf2","time":1706003363,"mediantime":1706000623,"nonce":104215386,"bits":"1d00ffff","difficulty":1,"chainwork":"000000000000000000000000000000000000000000000ca7ec1003d61879bb4f","nTx":2,"previousblockhash":"0000000000000007596e9d6ef8cb689081aa1a5429f8e5eb9c698b3850e10c9c","nextblockhash":"000000000000001a5a90462e80afc4540e02cd614172087843448b2ee0ecbb7d","strippedsize":259,"size":295,"weight":1072,"tx":[{"txid":"83ea376943aec104539ebd68a8dc23e3850896a93662c7032efd1c2eb22d9279","hash":"81e0923655f882241d9114ccfd787b9f56ad022eec7a96e4ddc0c2c591cf1c56","version":2,"size":149,"vsize":122,"weight":488,"locktime":0,"vin":[{"coinbase":"03274c2700","txinwitness":["0000000000000000000000000000000000000000000000000000000000000000"],"sequence":4294967295}],"vout":[{"value":0.01220703,"n":0,"scriptPubKey":{"asm":"1","desc":"raw(51)#8lvh9jxk","hex":"51","type":"nonstandard"}},{"value":0.00000000,"n":1,"scriptPubKey":{"asm":"OP_RETURN aa21a9ed50aae21ecb1c76811929da5971b501b393b68d3b0fc722366651c7f157ad4ae9","desc":"raw(6a24aa21a9ed50aae21ecb1c76811929da5971b501b393b68d3b0fc722366651c7f157ad4ae9)#gyrgewjq","hex":"6a24aa21a9ed50aae21ecb1c76811929da5971b501b393b68d3b0fc722366651c7f157ad4ae9","type":"nulldata"}}],"hex":"020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff0503274c2700ffffffff025fa012000000000001510000000000000000266a24aa21a9ed50aae21ecb1c76811929da5971b501b393b68d3b0fc722366651c7f157ad4ae90120000000000000000000000000000000000000000000000000000000000000000000000000"},{"txid":"5839f20446d7b9446e82c00117ee3699fa84154e970d57f09add60deef2eaa18","hash":"5839f20446d7b9446e82c00117ee3699fa84154e970d57f09add60deef2eaa18","version":4294967295,"size":65,"vsize":65,"weight":260,"locktime":0,"vin":[{"txid":"29e8adaf19cbc2b5dfdfc04eba3e73b47203c6bd1243dd9905843ca31b718973","vout":0,"scriptSig":{"asm":"1 OP_CHECKSEQUENCEVERIFY","hex":"51b2"},"sequence":1}],"vout":[{"value":0.00000000,"n":0,"scriptPubKey":{"asm":"OP_RETURN -42","desc":"raw(6a01aa)#fae3nleu","hex":"6a01aa","type":"nulldata"}}],"fee":0.00065535,"hex":"ffffffff017389711ba33c840599dd4312bdc60372b4733eba4ec0dfdfb5c2cb19afade829000000000251b201000000010000000000000000036a01aa00000000"}]}"#;
		let jd = &mut serde_json::Deserializer::from_str(block_data);
		let _result: VerboseBlock = serde_path_to_error::deserialize(jd).unwrap();
	}

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
			None,
		)
		.unwrap()
		.await;

		// from a `getblock` RPC call with verbosity 2
		let block_data: &str = r#"{"hash":"7f03b5690dffca7e446fa986df8c2269389e5846b4e5e8942e8230238392cc55","confirmations":235,"height":113,"version":536870912,"versionHex":"20000000","merkleroot":"3eb919e3d8be10620dd18daa33578859fd4ee8fcfa7d182ad33ef5b69e9347aa","time":1702309098,"mediantime":1702309073,"nonce":0,"bits":"207fffff","difficulty":4.656542373906925e-10,"chainwork":"00000000000000000000000000000000000000000000000000000000000000e4","nTx":2,"previousblockhash":"524d0847614eb89dffd4291786c111473fe7ca3334c7fddd2fa399481a2a99e7","nextblockhash":"7f3bf60a2dc7f2726b7afd5b7fc04d8626edd174779e16c6a3dfa6a450758156","strippedsize":332,"size":477,"weight":1473,"tx":[{"txid":"e2056652441becbfe33cc3b51c70a804360bdf1a730e65f0ee9abbe4028bf8f2","hash":"596da34ae4bb652af9872990ffe6746b922cee46e5fee25f8a16b9e6c3d17c73","version":2,"size":168,"vsize":141,"weight":564,"locktime":0,"vin":[{"coinbase":"017100","txinwitness":["0000000000000000000000000000000000000000000000000000000000000000"],"sequence":4294967295}],"vout":[{"value":50.00000147,"n":0,"scriptPubKey":{"asm":"0 a66802f0279cc06c04abe451733d0644b7cd1aa3","desc":"addr(bcrt1q5e5q9up8nnqxcp9tu3ghx0gxgjmu6x4rkhnr04)#qqa77u3g","hex":"0014a66802f0279cc06c04abe451733d0644b7cd1aa3","address":"bcrt1q5e5q9up8nnqxcp9tu3ghx0gxgjmu6x4rkhnr04","type":"witness_v0_keyhash"}},{"value":0.00000000,"n":1,"scriptPubKey":{"asm":"OP_RETURN aa21a9ed9b0c92763f407d12a24735fbdb9e720367e70b0117030e8bc648a5c03ebd4a5d","desc":"raw(6a24aa21a9ed9b0c92763f407d12a24735fbdb9e720367e70b0117030e8bc648a5c03ebd4a5d)#8sr8jfxw","hex":"6a24aa21a9ed9b0c92763f407d12a24735fbdb9e720367e70b0117030e8bc648a5c03ebd4a5d","type":"nulldata"}}],"hex":"020000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff03017100ffffffff0293f2052a01000000160014a66802f0279cc06c04abe451733d0644b7cd1aa30000000000000000266a24aa21a9ed9b0c92763f407d12a24735fbdb9e720367e70b0117030e8bc648a5c03ebd4a5d0120000000000000000000000000000000000000000000000000000000000000000000000000"},{"txid":"7b43fc408a6b1eb61f312b6732c27a2dde77828de7a677157d1ea03f7888748a","hash":"47d0b049a25be39a6f5b7871d9c6f9350e945095ba09aa3290494f399a4c66f7","version":2,"size":228,"vsize":147,"weight":585,"locktime":112,"vin":[{"txid":"773171b0c840f1dc69c247db64efc553dc2ce0c18558cd9ecef8e7bf81a81f2d","vout":0,"scriptSig":{"asm":"","hex":""},"txinwitness":["30440220379b67fd1ac79d3416aad215b9185144430a71c5f530cafcc076fe4df3025c75022045f823e4d2f15a5daa89ee1682a15f42b7cd95f6594b34315589676aab92203901","03b22186e3b2c239cb47066fca1f42b416772a27a113ba2c968bba069354f7c20a"],"sequence":4294967293}],"vout":[{"value":39.99999853,"n":0,"scriptPubKey":{"asm":"OP_DUP OP_HASH160 0b308fdc0ae8b11dde0673387ac36f20100bf5e9 OP_EQUALVERIFY OP_CHECKSIG","desc":"addr(mgY7uJoK6HszJqYxcwAKpJFj56bEJiuAhB)#pu2vn08p","hex":"76a9140b308fdc0ae8b11dde0673387ac36f20100bf5e988ac","address":"mgY7uJoK6HszJqYxcwAKpJFj56bEJiuAhB","type":"pubkeyhash"}},{"value":10.00000000,"n":1,"scriptPubKey":{"asm":"OP_DUP OP_HASH160 9a1c78a507689f6f54b847ad1cef1e614ee23f1e OP_EQUALVERIFY OP_CHECKSIG","desc":"addr(muZpTpBYhxmRFuCjLc7C6BBDF32C8XVJUi)#t9q9hu3x","hex":"76a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac","address":"muZpTpBYhxmRFuCjLc7C6BBDF32C8XVJUi","type":"pubkeyhash"}}],"fee":0.00000147,"hex":"020000000001012d1fa881bfe7f8ce9ecd5885c1e02cdc53c5ef64db47c269dcf140c8b07131770000000000fdffffff026d276bee000000001976a9140b308fdc0ae8b11dde0673387ac36f20100bf5e988ac00ca9a3b000000001976a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac024730440220379b67fd1ac79d3416aad215b9185144430a71c5f530cafcc076fe4df3025c75022045f823e4d2f15a5daa89ee1682a15f42b7cd95f6594b34315589676aab922039012103b22186e3b2c239cb47066fca1f42b416772a27a113ba2c968bba069354f7c20a70000000"}]}"#;
		let jd = &mut serde_json::Deserializer::from_str(block_data);

		let result: Result<VerboseBlock, _> = serde_path_to_error::deserialize(jd);

		match result {
			Ok(vb) => {
				println!("vb: {vb:?}");
				println!("Verbose block fee: {}", vb.txdata[1].fee.unwrap());
			},
			Err(e) => panic!("error: {e:?}"),
		}

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

		let v_block = client.block(best_block_hash).await.unwrap();

		println!("verbose block: {v_block:?}");

		println!("number of txs: {}", v_block.txdata.len());

		let tx = &v_block.txdata[0];

		let raw_transaction = client.get_raw_transactions(vec![tx.txid]).await.unwrap()[0].txid();
		assert_eq!(raw_transaction, tx.txid);

		// let average_block_fee_rate =
		// client.average_block_fee_rate(best_block_hash).await.unwrap();

		// println!("average_block_fee_rate: {average_block_fee_rate}");

		// Generate new hex bytes using ./bouncer/commands/create_raw_btc_tx.ts;
		// let hex_str =
		// "0200000000010133e287d3a464b226a1917303e1714af508a6bfe219265184a93c7f78851085a30000000000fdffffff0200e1f505000000001976a9149a1c78a507689f6f54b847ad1cef1e614ee23f1e88ac58d94a1f000000001600145baa7941ea1268fbd6279a0408a0419f8acd8245024730440220590dcc64661a362b54543f66d3cd24fdeae8a210643ad4f6fd39031281a9657902201f7a750d01f7cfc948ae4f8b76221b5c325f8ff82ac1b0d1d9a927632b40dd6001210386dc234ecbc4e677b927da260349cbd399c622507feb9dd2895a3537f6d4aa5d00000000";

		// let bytes = hex::decode(hex_str).unwrap();

		// let send_raw_transaction = client.send_raw_transaction(bytes).await.unwrap();

		// println!("tx_id: {:?}", send_raw_transaction);

		// let best_
	}
}
