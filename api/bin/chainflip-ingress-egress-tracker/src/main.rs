use chainflip_engine::settings::{HttpBasicAuthEndpoint, WsHttpEndpoints};
use futures::FutureExt;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use std::{env, net::SocketAddr};
use tracing::log;
use utilities::task_scope;

mod witnessing;

#[derive(Clone)]
pub struct DepositTrackerSettings {
	eth_node: WsHttpEndpoints,
	dot_node: WsHttpEndpoints,
	state_chain_ws_endpoint: String,
	btc: HttpBasicAuthEndpoint,
}

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");
	let mut module = RpcModule::new(());

	let btc_tracker = witnessing::btc_mempool::start(scope, settings.btc.clone()).await;

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

	// Broadcast channel will drop old messages when the buffer is full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	witnessing::start(scope, settings, witness_sender.clone()).await?;

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

	scope.spawn(async {
		let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
		let addr = server.local_addr()?;
		log::info!("Listening on http://{}", addr);
		server.start(module)?.stopped().await;
		// If the server stops for some reason, we return
		// error to terminate other tasks and the process.
		Err(anyhow::anyhow!("RPC server stopped"))
	});

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let settings = DepositTrackerSettings {
		eth_node: WsHttpEndpoints {
			ws_endpoint: env::var("ETH_WS_ENDPOINT")
				.unwrap_or("ws://localhost:8546".to_string())
				.into(),
			http_endpoint: env::var("ETH_HTTP_ENDPOINT")
				.unwrap_or("http://localhost:8545".to_string())
				.into(),
		},
		dot_node: WsHttpEndpoints {
			ws_endpoint: env::var("DOT_WS_ENDPOINT")
				.unwrap_or("ws://localhost:9947".to_string())
				.into(),
			http_endpoint: env::var("DOT_HTTP_ENDPOINT")
				.unwrap_or("http://localhost:9947".to_string())
				.into(),
		},
		state_chain_ws_endpoint: env::var("SC_WS_ENDPOINT")
			.unwrap_or("ws://localhost:9944".to_string()),
		btc: HttpBasicAuthEndpoint {
			http_endpoint: env::var("BTC_ENDPOINT")
				.unwrap_or("http://127.0.0.1:8332".to_string())
				.into(),
			basic_auth_user: env::var("BTC_USERNAME").unwrap_or("flip".to_string()),
			basic_auth_password: env::var("BTC_PASSWORD").unwrap_or("flip".to_string()),
		},
	};

	task_scope::task_scope(|scope| async move { start(scope, settings).await }.boxed()).await
}
