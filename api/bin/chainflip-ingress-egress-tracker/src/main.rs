use chainflip_engine::settings::WsHttpEndpoints;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use std::{env, io::Write, net::SocketAddr, path::PathBuf};
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
	let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
	let mut module = RpcModule::new(());

	let btc_tracker = witnessing::btc::start().await;

	module.register_async_method("status", move |arguments, _context| {
		let btc_tracker = btc_tracker.clone();
		async move {
			arguments.parse::<Vec<String>>().map_err(Error::Call).and_then(|addresses| {
				btc_tracker
					.lookup_transactions(&addresses)
					.map_err(|err| jsonrpsee::core::Error::Custom(err.to_string()))
			})
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
	let _ = serverhandle.await;
	Ok(())
}
