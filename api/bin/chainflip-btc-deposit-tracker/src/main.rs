use anyhow::anyhow;
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

#[derive(Deserialize)]
struct ScriptPubKey {
	address: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vout {
	value: f64,
	script_pub_key: ScriptPubKey,
}

#[derive(Deserialize)]
struct RawTx {
	txid: String,
	vout: Vec<Vout>,
}

#[derive(Deserialize)]
struct RawTxResult {
	result: RawTx,
}

#[derive(Deserialize)]
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
	value: f64,
	tx_hash: String,
}

#[derive(Default, Clone)]
struct Cache {
	best_block_hash: String,
	transactions: HashMap<String, QueryResult>,
}

const SAFETY_MARGIN: u32 = 7;
const REFRESH_INTERVAL: u64 = 10;

async fn btc_call<T: DeserializeOwned>(method: &str, params: &str) -> anyhow::Result<T> {
	let url = env::var("BTC_ENDPOINT").unwrap_or("http://127.0.0.1:8332".to_string());
	reqwest::Client::new()
		.post(url)
		.header("Content-Type", "text/plain")
		.body(format!(r#"{{"jsonrpc":"1.0","id":0,"method":"{}","params":[{}]}}"#, method, params))
		.basic_auth("flip", Some("flip"))
		.send()
		.await?
		.json::<T>()
		.await
		.map_err(|err| anyhow!(err))
}

async fn get_updated_cache(current_cache: Cache) -> anyhow::Result<Cache> {
	let mempool = btc_call::<MemPoolResult>("getrawmempool", "").await?.result;
	let mut cache: HashMap<String, QueryResult> = Default::default();
	for tx_hash in mempool {
		let vouts = btc_call::<RawTxResult>(
			"getrawtransaction",
			format!("\"{}\", true", tx_hash.clone()).as_str(),
		)
		.await
		.map(|tx| tx.result.vout)
		// Don't error here. It could be that the transaction was already removed from the mempool
		// by the time we tried to query it.
		.unwrap_or_default();
		for vout in vouts {
			if let Some(destination) = vout.script_pub_key.address {
				cache.insert(
					destination,
					QueryResult { confirmations: 0, value: vout.value, tx_hash: tx_hash.clone() },
				);
			}
		}
	}

	let block_hash = btc_call::<BestBlockResult>("getbestblockhash", "").await?.result;
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
			let block = btc_call::<BlockResult>(
				"getblock",
				format!("\"{}\", 2", block_hash_to_query).as_str(),
			)
			.await?
			.result;
			for tx in block.tx {
				for vout in tx.vout {
					if let Some(destination) = vout.script_pub_key.address {
						cache.insert(
							destination,
							QueryResult {
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
	Ok(Cache { best_block_hash: block_hash, transactions: cache })
}

fn lookup_transactions(cache: Cache, addresses: Vec<String>) -> Vec<Option<QueryResult>> {
	addresses
		.iter()
		.map(|address| cache.transactions.get(address).map(Clone::clone))
		.collect::<Vec<Option<QueryResult>>>()
}

enum CacheStatus {
	Init,
	Ready,
	Down,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");
	let cache: Arc<Mutex<Cache>> = Default::default();
	let (btc_status_sender, btc_status_receiver) = tokio::sync::watch::channel(CacheStatus::Init);
	let updater = task::spawn({
		let cache = cache.clone();
		async move {
			let mut interval = time::interval(Duration::from_secs(REFRESH_INTERVAL));
			interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
			loop {
				interval.tick().await;
				let cache_copy = cache.lock().unwrap().clone();
				match get_updated_cache(cache_copy).await {
					Ok(updated_cache) => {
						let mut cache = cache.lock().unwrap();
						*cache = updated_cache;
						btc_status_sender.send(CacheStatus::Ready).unwrap();
					},
					Err(err) => {
						log::error!("Error when querying Bitcoin chain: {}", err);
						btc_status_sender.send(CacheStatus::Down).unwrap();
					},
				}
			}
		}
	});
	let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
	let mut module = RpcModule::new(());
	module.register_async_method("status", move |arguments, _context| {
		let cache = cache.clone();
		let btc_status_receiver = btc_status_receiver.clone();
		async move {
			arguments
				.parse::<Vec<String>>()
				.and_then(|addresses| match *btc_status_receiver.borrow() {
					CacheStatus::Ready =>
						Ok(lookup_transactions(cache.lock().unwrap().clone(), addresses)),
					CacheStatus::Init => Err(anyhow!("Address cache is not intialised.").into()),
					CacheStatus::Down =>
						Err(anyhow!("Address cache is down - check btc connection.").into()),
				})
				.map_err(Error::Call)
		}
	})?;
	let addr = server.local_addr()?;
	log::info!("Listening on http://{}", addr);
	let serverhandle = Box::pin(server.start(module)?.stopped());
	let _ = future::select(serverhandle, updater).await;
	Ok(())
}
