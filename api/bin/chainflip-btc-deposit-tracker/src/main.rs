use anyhow::anyhow;
use futures::future;
use jsonrpsee::{server::Server, RpcModule};
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

const SAFETY_MARGIN: u32 = 7;

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

async fn get_updated_cache() -> anyhow::Result<HashMap<String, QueryResult>> {
    let mempool = btc_call::<MemPoolResult>("getrawmempool", "").await?.result;
    let mut cache: HashMap<String, QueryResult> = Default::default();
    for tx_hash in mempool {
        let vouts = btc_call::<RawTxResult>(
            "getrawtransaction",
            format!("\"{}\", true", tx_hash.clone()).as_str(),
        )
        .await?
        .result
        .vout;
        for vout in vouts {
            if let Some(destination) = vout.script_pub_key.address {
                cache.insert(
                    destination,
                    QueryResult { confirmations: 0, value: vout.value, tx_hash: tx_hash.clone() },
                );
            }
        }
    }

    let mut block_hash = btc_call::<BestBlockResult>("getbestblockhash", "").await?.result;
    for confirmations in 1..SAFETY_MARGIN {
        let block = btc_call::<BlockResult>("getblock", format!("\"{}\", 2", block_hash).as_str())
            .await?
            .result;
        for tx in block.tx {
            for vout in tx.vout {
                if let Some(destination) = vout.script_pub_key.address {
                    cache.insert(
                        destination,
                        QueryResult { confirmations, value: vout.value, tx_hash: tx.txid.clone() },
                    );
                }
            }
        }
        block_hash = block.previousblockhash;
    }
    Ok(cache)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .expect("setting default subscriber failed");
    let cache: Arc<Mutex<HashMap<String, QueryResult>>> = Default::default();
    let updater = task::spawn({
        let cache = cache.clone();
        async move {
            let mut interval = time::interval(Duration::from_secs(10));
            loop {
                match get_updated_cache().await {
                    Ok(updated_cache) => {
                        let mut cache = cache.lock().unwrap();
                        *cache = updated_cache;
                    },
                    anyhow::Result::Err(err) => {
                        log::error!("Error when querying Bitcoin chain: {}", err);
                    },
                }
                interval.tick().await;
            }
        }
    });
    let server = Server::builder().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
    let mut module = RpcModule::new(());
    module.register_async_method("status", move |arguments, _context| {
        let cache = cache.clone();
        async move {
            arguments
                .parse::<String>()
                .map(|address| cache.lock().unwrap().get(&address).cloned())
        }
    })?;
    let addr = server.local_addr()?;
    log::info!("Listening on http://{}", addr);
    let handle = server.start(module);
    let _ = future::join(handle.stopped(), updater).await;
    Ok(())
}
