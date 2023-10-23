use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
	time::Duration,
};

use anyhow::anyhow;
use async_trait::async_trait;
use chainflip_engine::settings::HttpBasicAuthEndpoint;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{error, info};
use utilities::task_scope;

type TxHash = String;
type BlockHash = String;
type Address = String;

#[derive(Deserialize)]
struct BestBlockResult {
	result: BlockHash,
}

#[derive(Deserialize)]
struct MemPoolResult {
	result: Vec<TxHash>,
}

#[derive(Deserialize, Clone)]
struct ScriptPubKey {
	address: Option<Address>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Vout {
	value: f64,
	script_pub_key: ScriptPubKey,
}

#[derive(Deserialize, Clone)]
struct RawTx {
	txid: TxHash,
	vout: Vec<Vout>,
}

#[derive(Deserialize)]
struct RawTxResult {
	result: Option<RawTx>,
}

#[derive(Deserialize, Clone)]
struct Block {
	previousblockhash: BlockHash,
	tx: Vec<RawTx>,
}

#[derive(Deserialize)]
struct BlockResult {
	result: Block,
}

#[derive(Clone, Serialize)]
pub struct QueryResult {
	confirmations: u32,
	destination: Address,
	value: f64,
	tx_hash: TxHash,
}

#[derive(Default, Clone)]
enum CacheStatus {
	#[default]
	Init,
	Ready,
	Down,
}

#[derive(Default, Clone)]
struct Cache {
	status: CacheStatus,
	best_block_hash: BlockHash,
	transactions: HashMap<Address, QueryResult>,
	known_tx_hashes: HashSet<TxHash>,
}

const SAFETY_MARGIN: u32 = 10;
const REFRESH_INTERVAL: u64 = 10;

#[async_trait]
trait BtcNode {
	async fn getrawmempool(&self) -> anyhow::Result<Vec<TxHash>>;
	async fn getrawtransactions(
		&self,
		tx_hashes: Vec<TxHash>,
	) -> anyhow::Result<Vec<Option<RawTx>>>;
	async fn getbestblockhash(&self) -> anyhow::Result<BlockHash>;
	async fn getblock(&self, block_hash: BlockHash) -> anyhow::Result<Block>;
}

struct BtcRpc {
	settings: HttpBasicAuthEndpoint,
}

impl BtcRpc {
	async fn call<T: DeserializeOwned>(
		&self,
		method: &str,
		params: Vec<&str>,
	) -> anyhow::Result<Vec<T>> {
		info!("Calling {} with batch size of {}", method, params.len());
		let body = params
			.iter()
			.map(|param| {
				format!(r#"{{"jsonrpc":"1.0","id":0,"method":"{}","params":[{}]}}"#, method, param)
			})
			.collect::<Vec<String>>()
			.join(",");

		reqwest::Client::new()
			.post(self.settings.http_endpoint.as_ref())
			.basic_auth(&self.settings.basic_auth_user, Some(&self.settings.basic_auth_password))
			.header("Content-Type", "text/plain")
			.body(format!("[{}]", body))
			.send()
			.await?
			.json::<Vec<T>>()
			.await
			.map_err(|err| anyhow!(err))
			.and_then(|result| {
				if result.len() == params.len() {
					Ok(result)
				} else {
					Err(anyhow!("Batched request returned an incorrect number of results"))
				}
			})
	}
}

#[async_trait]
impl BtcNode for BtcRpc {
	async fn getrawmempool(&self) -> anyhow::Result<Vec<TxHash>> {
		self.call::<MemPoolResult>("getrawmempool", vec![""])
			.await
			.map(|x| x[0].result.clone())
	}
	async fn getrawtransactions(
		&self,
		tx_hashes: Vec<TxHash>,
	) -> anyhow::Result<Vec<Option<RawTx>>> {
		let params = tx_hashes
			.iter()
			.map(|tx_hash| format!("\"{}\",  true", tx_hash))
			.collect::<Vec<String>>();
		Ok(self
			.call::<RawTxResult>(
				"getrawtransaction",
				params.iter().map(|x| x.as_str()).collect::<Vec<&str>>(),
			)
			.await?
			.into_iter()
			.map(|x| x.result)
			.collect::<Vec<Option<RawTx>>>())
	}
	async fn getbestblockhash(&self) -> anyhow::Result<BlockHash> {
		self.call::<BestBlockResult>("getbestblockhash", vec![""])
			.await
			.map(|x| x[0].result.clone())
	}
	async fn getblock(&self, block_hash: String) -> anyhow::Result<Block> {
		self.call::<BlockResult>("getblock", vec![&format!("\"{}\", 2", block_hash)])
			.await
			.map(|x| x[0].result.clone())
	}
}

async fn get_updated_cache<T: BtcNode>(btc: &T, previous_cache: Cache) -> anyhow::Result<Cache> {
	let all_mempool_transactions: Vec<TxHash> = btc.getrawmempool().await?;
	let mut new_transactions: HashMap<Address, QueryResult> = Default::default();
	let mut new_known_tx_hashes: HashSet<TxHash> = Default::default();
	let previous_mempool: HashMap<TxHash, QueryResult> = previous_cache
		.clone()
		.transactions
		.into_iter()
		.filter_map(|(_, query_result)| {
			if query_result.confirmations == 0 {
				Some((query_result.tx_hash.clone(), query_result))
			} else {
				None
			}
		})
		.collect();
	let unknown_mempool_transactions: Vec<TxHash> = all_mempool_transactions
		.into_iter()
		.filter(|tx_hash| {
			if let Some(known_transaction) = previous_mempool.get(tx_hash) {
				new_known_tx_hashes.insert(tx_hash.clone());
				new_transactions
					.insert(known_transaction.destination.clone(), known_transaction.clone());
			} else if previous_cache.known_tx_hashes.contains(tx_hash) {
				new_known_tx_hashes.insert(tx_hash.clone());
			} else {
				return true
			}
			false
		})
		.collect();
	let transactions: Vec<RawTx> = btc
		.getrawtransactions(unknown_mempool_transactions)
		.await?
		.iter()
		.filter_map(|x| x.clone())
		.collect();
	for tx in transactions {
		for vout in tx.vout {
			new_known_tx_hashes.insert(tx.txid.clone());
			if let Some(destination) = vout.script_pub_key.address {
				new_transactions.insert(
					destination.clone(),
					QueryResult {
						destination,
						confirmations: 0,
						value: vout.value,
						tx_hash: tx.txid.clone(),
					},
				);
			}
		}
	}
	let block_hash = btc.getbestblockhash().await?;
	if previous_cache.best_block_hash == block_hash {
		for entry in previous_cache.transactions {
			if entry.1.confirmations > 0 {
				new_transactions.insert(entry.0, entry.1);
			}
		}
	} else {
		info!("New block found: {}", block_hash);
		let mut block_hash_to_query = block_hash.clone();
		for confirmations in 1..SAFETY_MARGIN {
			let block = btc.getblock(block_hash_to_query).await?;
			for tx in block.tx {
				for vout in tx.vout {
					if let Some(destination) = vout.script_pub_key.address {
						new_transactions.insert(
							destination.clone(),
							QueryResult {
								destination,
								confirmations,
								value: vout.value,
								tx_hash: tx.txid.clone(),
							},
						);
					}
				}
			}
			block_hash_to_query = block.previousblockhash;
		}
	}
	Ok(Cache {
		status: CacheStatus::Ready,
		best_block_hash: block_hash,
		transactions: new_transactions,
		known_tx_hashes: new_known_tx_hashes,
	})
}

fn lookup_transactions(
	cache: &Cache,
	addresses: &[String],
) -> anyhow::Result<Vec<Option<QueryResult>>> {
	match cache.status {
		CacheStatus::Ready => Ok(addresses
			.iter()
			.map(|address| cache.transactions.get(address).map(Clone::clone))
			.collect::<Vec<Option<QueryResult>>>()),
		CacheStatus::Init => Err(anyhow!("Address cache is not initialised.")),
		CacheStatus::Down => Err(anyhow!("Address cache is down - check btc connection.")),
	}
}

#[derive(Clone)]
pub struct BtcTracker {
	cache: Arc<Mutex<Cache>>,
}

impl BtcTracker {
	pub fn lookup_transactions(
		&self,
		addresses: &[String],
	) -> anyhow::Result<Vec<Option<QueryResult>>> {
		lookup_transactions(&self.cache.lock().unwrap(), addresses)
	}
}

pub async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	endpoint: HttpBasicAuthEndpoint,
) -> BtcTracker {
	let cache: Arc<Mutex<Cache>> = Default::default();
	scope.spawn({
		let cache = cache.clone();
		async move {
			let btc_rpc = BtcRpc { settings: endpoint };
			let mut interval = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL));
			interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
			loop {
				interval.tick().await;
				let cache_copy = cache.lock().unwrap().clone();
				match get_updated_cache(&btc_rpc, cache_copy).await {
					Ok(updated_cache) => {
						let mut cache = cache.lock().unwrap();
						*cache = updated_cache;
					},
					Err(err) => {
						error!("Error when querying Bitcoin chain: {}", err);
						let mut cache = cache.lock().unwrap();
						cache.status = CacheStatus::Down;
					},
				}
			}
		}
	});

	BtcTracker { cache }
}

#[cfg(test)]
mod tests {

	use super::*;

	#[derive(Clone)]
	struct MockBtcRpc {
		mempool: Vec<RawTx>,
		latest_block_hash: String,
		blocks: HashMap<String, Block>,
	}

	#[async_trait]
	impl BtcNode for MockBtcRpc {
		async fn getrawmempool(&self) -> anyhow::Result<Vec<String>> {
			Ok(self.mempool.iter().map(|x| x.txid.clone()).collect())
		}
		async fn getrawtransactions(
			&self,
			tx_hashes: Vec<String>,
		) -> anyhow::Result<Vec<Option<RawTx>>> {
			let mut result: Vec<Option<RawTx>> = Default::default();
			for hash in tx_hashes {
				for tx in self.mempool.clone() {
					if tx.txid == hash {
						result.push(Some(tx))
					} else {
						result.push(None)
					}
				}
			}
			Ok(result)
		}
		async fn getbestblockhash(&self) -> anyhow::Result<String> {
			Ok(self.latest_block_hash.clone())
		}
		async fn getblock(&self, block_hash: String) -> anyhow::Result<Block> {
			self.blocks.get(&block_hash).cloned().ok_or(anyhow!("Block missing"))
		}
	}

	#[tokio::test]
	async fn multiple_outputs_in_one_tx() {
		let mempool = vec![RawTx {
			txid: "tx1".into(),
			vout: vec![
				Vout {
					value: 0.8,
					script_pub_key: ScriptPubKey { address: Some("address1".into()) },
				},
				Vout {
					value: 1.2,
					script_pub_key: ScriptPubKey { address: Some("address2".into()) },
				},
			],
		}];
		let latest_block_hash = "15".to_string();
		let mut blocks: HashMap<String, Block> = Default::default();
		for i in 1..16 {
			blocks.insert(
				i.to_string(),
				Block { previousblockhash: (i - 1).to_string(), tx: vec![] },
			);
		}
		let btc = MockBtcRpc { mempool, latest_block_hash, blocks };
		let cache: Cache = Default::default();
		let cache = get_updated_cache(&btc, cache).await.unwrap();
		let result = lookup_transactions(&cache, &["address1".into(), "address2".into()]).unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[1].as_ref().unwrap().destination, "address2".to_string());
	}

	#[tokio::test]
	async fn mempool_updates() {
		let mempool = vec![
			RawTx {
				txid: "tx1".into(),
				vout: vec![Vout {
					value: 0.8,
					script_pub_key: ScriptPubKey { address: Some("address1".into()) },
				}],
			},
			RawTx {
				txid: "tx2".into(),
				vout: vec![Vout {
					value: 0.8,
					script_pub_key: ScriptPubKey { address: Some("address2".into()) },
				}],
			},
		];
		let latest_block_hash = "15".to_string();
		let mut blocks: HashMap<String, Block> = Default::default();
		for i in 1..16 {
			blocks.insert(
				i.to_string(),
				Block { previousblockhash: (i - 1).to_string(), tx: vec![] },
			);
		}
		let mut btc = MockBtcRpc { mempool: mempool.clone(), latest_block_hash, blocks };
		let cache: Cache = Default::default();
		let cache = get_updated_cache(&btc, cache).await.unwrap();
		let result =
			lookup_transactions(&cache, &["address1".into(), "address2".into(), "address3".into()])
				.unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[1].as_ref().unwrap().destination, "address2".to_string());
		assert!(result[2].is_none());

		btc.mempool.append(&mut vec![RawTx {
			txid: "tx3".into(),
			vout: vec![Vout {
				value: 0.8,
				script_pub_key: ScriptPubKey { address: Some("address3".into()) },
			}],
		}]);
		let cache = get_updated_cache(&btc, cache.clone()).await.unwrap();
		let result =
			lookup_transactions(&cache, &["address1".into(), "address2".into(), "address3".into()])
				.unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[1].as_ref().unwrap().destination, "address2".to_string());
		assert_eq!(result[2].as_ref().unwrap().destination, "address3".to_string());

		btc.mempool.remove(0);
		let cache = get_updated_cache(&btc, cache.clone()).await.unwrap();
		let result =
			lookup_transactions(&cache, &["address1".into(), "address2".into(), "address3".into()])
				.unwrap();
		assert!(result[0].is_none());
		assert_eq!(result[1].as_ref().unwrap().destination, "address2".to_string());
		assert_eq!(result[2].as_ref().unwrap().destination, "address3".to_string());
	}

	#[tokio::test]
	async fn blocks() {
		let mempool = vec![];
		let latest_block_hash = "15".to_string();
		let mut blocks: HashMap<String, Block> = Default::default();
		for i in 1..19 {
			blocks.insert(
				i.to_string(),
				Block { previousblockhash: (i - 1).to_string(), tx: vec![] },
			);
		}
		blocks.insert(
			"15".to_string(),
			Block {
				previousblockhash: "14".to_string(),
				tx: vec![RawTx {
					txid: "tx1".into(),
					vout: vec![Vout {
						value: 12.5,
						script_pub_key: ScriptPubKey { address: Some("address1".into()) },
					}],
				}],
			},
		);
		let mut btc = MockBtcRpc { mempool: mempool.clone(), latest_block_hash, blocks };
		let cache: Cache = Default::default();
		let cache = get_updated_cache(&btc, cache).await.unwrap();
		let result = lookup_transactions(&cache, &["address1".into()]).unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[0].as_ref().unwrap().confirmations, 1);

		btc.latest_block_hash = "16".to_string();
		let cache = get_updated_cache(&btc, cache.clone()).await.unwrap();
		let result = lookup_transactions(&cache, &["address1".into()]).unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[0].as_ref().unwrap().confirmations, 2);
	}

	#[tokio::test]
	async fn report_oldest_tx_only() {
		let mempool = vec![RawTx {
			txid: "tx2".into(),
			vout: vec![Vout {
				value: 0.8,
				script_pub_key: ScriptPubKey { address: Some("address1".into()) },
			}],
		}];
		let latest_block_hash = "15".to_string();
		let mut blocks: HashMap<String, Block> = Default::default();
		for i in 1..16 {
			blocks.insert(
				i.to_string(),
				Block { previousblockhash: (i - 1).to_string(), tx: vec![] },
			);
		}
		blocks.insert(
			"13".to_string(),
			Block {
				previousblockhash: "12".to_string(),
				tx: vec![RawTx {
					txid: "tx1".into(),
					vout: vec![Vout {
						value: 12.5,
						script_pub_key: ScriptPubKey { address: Some("address1".into()) },
					}],
				}],
			},
		);
		let btc = MockBtcRpc { mempool: mempool.clone(), latest_block_hash, blocks };
		let cache: Cache = Default::default();
		let cache = get_updated_cache(&btc, cache).await.unwrap();
		let result = lookup_transactions(&cache, &["address1".into()]).unwrap();
		assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
		assert_eq!(result[0].as_ref().unwrap().confirmations, 3);
		assert_eq!(result[0].as_ref().unwrap().value, 12.5);
	}
}
