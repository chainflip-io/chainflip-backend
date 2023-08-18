use anyhow::anyhow;
use async_trait::async_trait;
use futures::future;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
	collections::HashMap,
	env,
	net::SocketAddr,
	sync::{Arc, Mutex},
	time::Duration,
};
use tokio::{task, time};
use tracing::log;

#[derive(Deserialize)]
struct BestBlockResult {
	result: String,
}

#[derive(Deserialize)]
struct MemPoolResult {
	result: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct ScriptPubKey {
	address: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Vout {
	value: f64,
	script_pub_key: ScriptPubKey,
}

#[derive(Deserialize, Clone)]
struct RawTx {
	txid: String,
	vout: Vec<Vout>,
}

#[derive(Deserialize)]
struct RawTxResult {
	result: RawTx,
}

#[derive(Deserialize, Clone)]
struct Block {
	previousblockhash: String,
	tx: Vec<RawTx>,
}

#[derive(Deserialize)]
struct BlockResult {
	result: Block,
}

#[derive(Clone, Serialize)]
struct QueryResult {
	confirmations: u32,
	destination: String,
	value: f64,
	tx_hash: String,
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
	best_block_hash: String,
	transactions: HashMap<String, QueryResult>,
}

const SAFETY_MARGIN: u32 = 7;
const REFRESH_INTERVAL: u64 = 10;

#[async_trait]
trait BtcNode {
	async fn getrawmempool(&self) -> anyhow::Result<Vec<String>>;
	async fn getrawtransaction(&self, tx_hash: String) -> anyhow::Result<RawTx>;
	async fn getbestblockhash(&self) -> anyhow::Result<String>;
	async fn getblock(&self, block_hash: String) -> anyhow::Result<Block>;
}

struct BtcRpc;

impl BtcRpc {
	async fn call<T: DeserializeOwned>(&self, method: &str, params: &str) -> anyhow::Result<T> {
		let url = env::var("BTC_ENDPOINT").unwrap_or("http://127.0.0.1:8332".to_string());
		reqwest::Client::new()
			.post(url)
			.header("Content-Type", "text/plain")
			.body(format!(
				r#"{{"jsonrpc":"1.0","id":0,"method":"{}","params":[{}]}}"#,
				method, params
			))
			.basic_auth("flip", Some("flip"))
			.send()
			.await?
			.json::<T>()
			.await
			.map_err(|err| anyhow!(err))
	}
}

#[async_trait]
impl BtcNode for BtcRpc {
	async fn getrawmempool(&self) -> anyhow::Result<Vec<String>> {
		self.call::<MemPoolResult>("getrawmempool", "").await.map(|x| x.result)
	}
	async fn getrawtransaction(&self, tx_hash: String) -> anyhow::Result<RawTx> {
		self.call::<RawTxResult>("getrawtransaction", &format!("\"{}\", true", tx_hash))
			.await
			.map(|x| x.result)
	}
	async fn getbestblockhash(&self) -> anyhow::Result<String> {
		self.call::<BestBlockResult>("getbestblockhash", "").await.map(|x| x.result)
	}
	async fn getblock(&self, block_hash: String) -> anyhow::Result<Block> {
		self.call::<BlockResult>("getblock", &format!("\"{}\", 2", block_hash))
			.await
			.map(|x| x.result)
	}
}

async fn get_updated_cache<T: BtcNode>(btc: T, current_cache: Cache) -> anyhow::Result<Cache> {
	let mempool = btc.getrawmempool().await?;
	let mut cache: HashMap<String, QueryResult> = Default::default();
	let previous_mempool = current_cache
		.clone()
		.transactions
		.iter()
		.filter_map(|(_, query_result)| {
			if query_result.confirmations == 0 {
				Some((query_result.tx_hash.clone(), query_result.clone()))
			} else {
				None
			}
		})
		.collect::<HashMap<String, QueryResult>>();
	for tx_hash in mempool {
		if let Some(known_transaction) = previous_mempool.get(&tx_hash) {
			cache.insert(known_transaction.destination.clone(), known_transaction.clone());
		} else {
			let vouts = btc
				.getrawtransaction(tx_hash.clone())
				.await
				.map(|tx| tx.vout)
				// Don't error here. It could be that the transaction was already removed from the
				// mempool by the time we tried to query it.
				.unwrap_or_default();
			for vout in vouts {
				if let Some(destination) = vout.script_pub_key.address {
					cache.insert(
						destination.clone(),
						QueryResult {
							destination,
							confirmations: 0,
							value: vout.value,
							tx_hash: tx_hash.clone(),
						},
					);
				}
			}
		}
	}

	let block_hash = btc.getbestblockhash().await?;
	if current_cache.best_block_hash == block_hash {
		for entry in current_cache.transactions {
			if entry.1.confirmations > 0 {
				cache.insert(entry.0, entry.1);
			}
		}
	} else {
		log::info!("New block found: {}", block_hash);
		let mut block_hash_to_query = block_hash.clone();
		for confirmations in 1..SAFETY_MARGIN {
			let block = btc.getblock(block_hash_to_query).await?;
			for tx in block.tx {
				for vout in tx.vout {
					if let Some(destination) = vout.script_pub_key.address {
						cache.insert(
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
	Ok(Cache { status: CacheStatus::Ready, best_block_hash: block_hash, transactions: cache })
}

fn lookup_transactions(
	cache: Cache,
	addresses: Vec<String>,
) -> Result<Vec<Option<QueryResult>>, Error> {
	match cache.status {
		CacheStatus::Ready => Ok(addresses
			.iter()
			.map(|address| cache.transactions.get(address).map(Clone::clone))
			.collect::<Vec<Option<QueryResult>>>()),
		CacheStatus::Init => Err(anyhow!("Address cache is not initialised.").into()),
		CacheStatus::Down => Err(anyhow!("Address cache is down - check btc connection.").into()),
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");
	let cache: Arc<Mutex<Cache>> = Default::default();
	let updater = task::spawn({
		let cache = cache.clone();
		async move {
			let mut interval = time::interval(Duration::from_secs(REFRESH_INTERVAL));
			interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
			loop {
				interval.tick().await;
				let cache_copy = cache.lock().unwrap().clone();
				match get_updated_cache(BtcRpc, cache_copy).await {
					Ok(updated_cache) => {
						let mut cache = cache.lock().unwrap();
						*cache = updated_cache;
					},
					Err(err) => {
						log::error!("Error when querying Bitcoin chain: {}", err);
						let mut cache = cache.lock().unwrap();
						cache.status = CacheStatus::Down;
					},
				}
			}
		}
	});
	let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
	let mut module = RpcModule::new(());
	module.register_async_method("status", move |arguments, _context| {
		let cache = cache.clone();
		async move {
			arguments
				.parse::<Vec<String>>()
				.map_err(Error::Call)
				.and_then(|addresses| lookup_transactions(cache.lock().unwrap().clone(), addresses))
		}
	})?;
	let addr = server.local_addr()?;
	log::info!("Listening on http://{}", addr);
	let serverhandle = Box::pin(server.start(module)?.stopped());
	let _ = future::select(serverhandle, updater).await;
	Ok(())
}

#[cfg(test)]
#[derive(Clone)]
struct MockBtcRpc {
	mempool: Vec<RawTx>,
	latest_block_hash: String,
	blocks: HashMap<String, Block>,
}

#[cfg(test)]
#[async_trait]
impl BtcNode for MockBtcRpc {
	async fn getrawmempool(&self) -> anyhow::Result<Vec<String>> {
		Ok(self.mempool.iter().map(|x| x.txid.clone()).collect())
	}
	async fn getrawtransaction(&self, tx_hash: String) -> anyhow::Result<RawTx> {
		for tx in self.mempool.clone() {
			if tx.txid == tx_hash {
				return Ok(tx)
			}
		}
		Err(anyhow!("Transaction missing"))
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
			Vout { value: 0.8, script_pub_key: ScriptPubKey { address: Some("address1".into()) } },
			Vout { value: 1.2, script_pub_key: ScriptPubKey { address: Some("address2".into()) } },
		],
	}];
	let latest_block_hash = "15".to_string();
	let mut blocks: HashMap<String, Block> = Default::default();
	for i in 1..16 {
		blocks.insert(i.to_string(), Block { previousblockhash: (i - 1).to_string(), tx: vec![] });
	}
	let btc = MockBtcRpc { mempool, latest_block_hash, blocks };
	let cache: Cache = Default::default();
	let cache = get_updated_cache(btc, cache).await.unwrap();
	let result = lookup_transactions(cache, vec!["address1".into(), "address2".into()]).unwrap();
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
		blocks.insert(i.to_string(), Block { previousblockhash: (i - 1).to_string(), tx: vec![] });
	}
	let mut btc = MockBtcRpc { mempool: mempool.clone(), latest_block_hash, blocks };
	let cache: Cache = Default::default();
	let cache = get_updated_cache(btc.clone(), cache).await.unwrap();
	let result = lookup_transactions(
		cache.clone(),
		vec!["address1".into(), "address2".into(), "address3".into()],
	)
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
	let cache = get_updated_cache(btc.clone(), cache.clone()).await.unwrap();
	let result = lookup_transactions(
		cache.clone(),
		vec!["address1".into(), "address2".into(), "address3".into()],
	)
	.unwrap();
	assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
	assert_eq!(result[1].as_ref().unwrap().destination, "address2".to_string());
	assert_eq!(result[2].as_ref().unwrap().destination, "address3".to_string());

	btc.mempool.remove(0);
	let cache = get_updated_cache(btc.clone(), cache.clone()).await.unwrap();
	let result = lookup_transactions(
		cache.clone(),
		vec!["address1".into(), "address2".into(), "address3".into()],
	)
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
		blocks.insert(i.to_string(), Block { previousblockhash: (i - 1).to_string(), tx: vec![] });
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
	let cache = get_updated_cache(btc.clone(), cache).await.unwrap();
	let result = lookup_transactions(cache.clone(), vec!["address1".into()]).unwrap();
	assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
	assert_eq!(result[0].as_ref().unwrap().confirmations, 1);

	btc.latest_block_hash = "16".to_string();
	let cache = get_updated_cache(btc.clone(), cache.clone()).await.unwrap();
	let result = lookup_transactions(cache.clone(), vec!["address1".into()]).unwrap();
	assert_eq!(result[0].as_ref().unwrap().destination, "address1".to_string());
	assert_eq!(result[0].as_ref().unwrap().confirmations, 2);
}
