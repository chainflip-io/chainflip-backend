use chainflip_engine::settings::WsHttpEndpoints;
use futures::future;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
	collections::{HashMap, HashSet},
	env,
	io::Write,
	net::SocketAddr,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};
use tokio::{task, time};
use tracing::log;

mod witnessing;

pub struct DepositTrackerSettings {
	eth_node: WsHttpEndpoints,
	// The key shouldn't be necessary, but the current witnesser wants this
	eth_key_path: PathBuf,
	state_chain_ws_endpoint: String,
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
	let server = ServerBuilder::default()
		// It seems that if the client doesn't unsubscribe correctly, a "connection"
		// won't be released, and we will eventually reach the limit, so we increase
		// as a way to mitigate this issue.
		// TODO: ensure that connections are always released
		.build("0.0.0.0:13337".parse::<SocketAddr>()?)
		.await?;
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

	// Broadcast channel will drop old messages when the buffer if full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	// Temporary hack: we don't actually use eth key, but the current witnesser is
	// expecting a path with a valid key, so we create a temporary dummy key file here:
	let mut eth_key_temp_file = tempfile::NamedTempFile::new()?;
	eth_key_temp_file
		.write_all(b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
		.unwrap();
	let eth_key_path = eth_key_temp_file.path();

	let eth_ws_endpoint = env::var("ETH_WS_ENDPOINT").unwrap_or("ws://localhost:8546".to_string());
	let eth_http_endpoint =
		env::var("ETH_HTTP_ENDPOINT").unwrap_or("http://localhost:8545".to_string());
	let sc_ws_endpoint = env::var("SC_WS_ENDPOINT").unwrap_or("ws://localhost:9944".to_string());

	let settings = DepositTrackerSettings {
		eth_node: WsHttpEndpoints {
			ws_endpoint: eth_ws_endpoint.into(),
			http_endpoint: eth_http_endpoint.into(),
		},
		eth_key_path: eth_key_path.into(),
		state_chain_ws_endpoint: sc_ws_endpoint,
	};

	witnessing::start(settings, witness_sender.clone());

	module.register_subscription(
		"subscribe_witnessing",
		"s_witnessing",
		"unsubscribe_witnessing",
		move |_params, mut sink, _context| {
			let mut witness_receiver = witness_sender.subscribe();

			tokio::spawn(async move {
				while let Ok(event) = witness_receiver.recv().await {
					use codec::Encode;
					if let Ok(false) = sink.send(&event.encode()) {
						log::debug!("Subscription is closed");
						break
					}
				}
			});
			Ok(())
		},
	)?;

	let addr = server.local_addr()?;
	log::info!("Listening on http://{}", addr);
	let serverhandle = Box::pin(server.start(module)?.stopped());
	let _ = future::select(serverhandle, updater).await;
	Ok(())
}
