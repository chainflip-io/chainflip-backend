use crate::{store::RedisStore, witnessing::state_chain::handle_call};
use chainflip_engine::settings::CfSettings;
use clap::Parser;
use futures::FutureExt;
use settings::{DepositTrackerSettings, TrackerOptions};
use std::ops::Deref;
use store::{Storable, Store};
use utilities::task_scope;

mod settings;
mod store;
mod utils;
mod witnessing;

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let client = redis::Client::open(settings.redis_url.clone()).unwrap();
	let mut store = RedisStore::new(client.get_multiplexed_tokio_connection().await?);

	// Broadcast channel will drop old messages when the buffer is full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	let (state_chain_client, env_params) =
		witnessing::start(scope, settings.clone(), witness_sender.clone()).await?;
	let btc_network = env_params.chainflip_network.into();
	witnessing::btc_mempool::start(scope, settings.btc, store.clone(), btc_network);
	let mut witness_receiver = witness_sender.subscribe();

	while let Ok(call) = witness_receiver.recv().await {
		handle_call(call, &mut store, env_params.chainflip_network, state_chain_client.deref())
			.await?
	}

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let settings = DepositTrackerSettings::load_settings_from_all_sources(
		// Not using the config root or settings dir.
		"".to_string(),
		"",
		TrackerOptions::parse(),
	)?;

	task_scope::task_scope(|scope| async move { start(scope, settings).await }.boxed()).await
}
