use anyhow::Context;
use cf_primitives::{AccountRole, SemVer};
use chainflip_engine::{
	btc::{rpc::BtcRpcClient, BtcBroadcaster},
	db::{KeyStore, PersistentKeyDB},
	dot::{http_rpc::DotHttpRpcClient, DotBroadcaster},
	eth::{broadcaster::EthBroadcaster, rpc::EthRpcClient},
	health, p2p,
	settings::{CommandLineOptions, Settings},
	state_chain_observer::{
		self,
		client::{
			extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
			storage_api::StorageApi,
		},
	},
	witness,
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use jsonrpsee_subxt::core::client::ClientT;
use multisig::{self, bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};
use std::sync::{atomic::AtomicBool, Arc};
use utilities::{
	make_periodic_tick,
	task_scope::{self, task_scope, ScopedJoinHandle},
	CachedStream,
};

lazy_static::lazy_static! {
	static ref CFE_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

fn is_compatible_with_runtime(runtime_compatibility_version: &SemVer) -> bool {
	CFE_VERSION.major == runtime_compatibility_version.major &&
		CFE_VERSION.minor == runtime_compatibility_version.minor
}

enum CfeStatus {
	Active(ScopedJoinHandle<()>),
	Idle,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	use_chainflip_account_id_encoding();

	let settings = Settings::new(CommandLineOptions::parse()).context("Error reading settings")?;

	// Note: the greeting should only be printed in normal mode (i.e. not for short-lived commands
	// like `--version`), so we execute it only after the settings have been parsed.
	utilities::print_starting!();

	task_scope(|scope| {
		async move {
			let mut start_logger_server_fn = Some(utilities::init_json_logger().await);

			// Note that we use jsonrpsee ^0.16 to prevent unwanted
			// warning when this ws client is disconnected
			let ws_rpc_client = jsonrpsee_subxt::ws_client::WsClientBuilder::default()
				.build(&settings.state_chain.ws_endpoint)
				.await?;

			let mut cfe_status = CfeStatus::Idle;

			let mut poll_interval = make_periodic_tick(std::time::Duration::from_secs(6), true);
			loop {
				poll_interval.tick().await;

				let runtime_compatibility_version: SemVer = ws_rpc_client
					.request("cf_current_compatibility_version", Vec::<()>::new())
					.await
					.unwrap();

				let compatible =
					is_compatible_with_runtime(&runtime_compatibility_version);

				match cfe_status {
					CfeStatus::Active(_) =>
						if !compatible {
							tracing::info!(
								"Runtime version ({runtime_compatibility_version:?}) is no longer compatible, shutting down the engine!"
							);
							// This will exit the scope, dropping the handle and thus terminating
							// the main task
							break Err(anyhow::anyhow!("Incompatible runtime version"))
						},
					CfeStatus::Idle =>
						if compatible {
							start_logger_server_fn.take().expect("only called once")(scope);
							tracing::info!("Runtime version ({runtime_compatibility_version:?}) is compatible, starting the engine!");

							let settings = settings.clone();
							let handle = scope.spawn_with_handle(
								task_scope(|scope| start(scope, settings).boxed())
							);

							cfe_status = CfeStatus::Active(handle);
						} else {
							tracing::info!("Current runtime is not compatible with this CFE version ({:?})", *CFE_VERSION);
						}
				}
			}
		}
		.boxed()
	})
	.await
}

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: Settings,
) -> anyhow::Result<()> {
	let has_completed_initialising = Arc::new(AtomicBool::new(false));

	if let Some(health_check_settings) = &settings.health_check {
		health::start(scope, health_check_settings, has_completed_initialising.clone()).await?;
	}

	let (state_chain_stream, state_chain_client) =
		state_chain_observer::client::StateChainClient::connect_with_account(
			scope,
			&settings.state_chain.ws_endpoint,
			&settings.state_chain.signing_key_file,
			AccountRole::Validator,
			true,
		)
		.await?;

	let btc_rpc_client =
		BtcRpcClient::new(&settings.btc).context("Failed to create Bitcoin Client")?;

	state_chain_client
		.submit_signed_extrinsic(pallet_cf_validator::Call::cfe_version {
			new_version: *CFE_VERSION,
		})
		.await
		.until_finalized()
		.await
		.context("Failed to submit version to state chain")?;

	let db = Arc::new(
		PersistentKeyDB::open_and_migrate_to_latest(
			settings.signing.db_file.as_path(),
			Some(state_chain_client.genesis_hash()),
		)
		.context("Failed to open database")?,
	);

	let (
		eth_outgoing_sender,
		eth_incoming_receiver,
		dot_outgoing_sender,
		dot_incoming_receiver,
		btc_outgoing_sender,
		btc_incoming_receiver,
		peer_update_sender,
		p2p_fut,
	) = p2p::start(
		state_chain_client.clone(),
		settings.node_p2p.clone(),
		state_chain_stream.cache().block_hash,
	)
	.await
	.context("Failed to start p2p")?;

	scope.spawn(p2p_fut);

	let (eth_multisig_client, eth_multisig_client_backend_future) =
		chainflip_engine::multisig::start_client::<EthSigning>(
			state_chain_client.account_id(),
			KeyStore::new(db.clone()),
			eth_incoming_receiver,
			eth_outgoing_sender,
			state_chain_client
				.storage_value::<pallet_cf_vaults::CeremonyIdCounter<
					state_chain_runtime::Runtime,
					state_chain_runtime::EthereumInstance,
				>>(state_chain_stream.cache().block_hash)
				.await
				.context("Failed to get Ethereum CeremonyIdCounter from SC")?,
		);

	scope.spawn(eth_multisig_client_backend_future);

	let (dot_multisig_client, dot_multisig_client_backend_future) =
		chainflip_engine::multisig::start_client::<PolkadotSigning>(
			state_chain_client.account_id(),
			KeyStore::new(db.clone()),
			dot_incoming_receiver,
			dot_outgoing_sender,
			state_chain_client
				.storage_value::<pallet_cf_vaults::CeremonyIdCounter<
					state_chain_runtime::Runtime,
					state_chain_runtime::PolkadotInstance,
				>>(state_chain_stream.cache().block_hash)
				.await
				.context("Failed to get Polkadot CeremonyIdCounter from SC")?,
		);

	scope.spawn(dot_multisig_client_backend_future);

	let (btc_multisig_client, btc_multisig_client_backend_future) =
		chainflip_engine::multisig::start_client::<BtcSigning>(
			state_chain_client.account_id(),
			KeyStore::new(db.clone()),
			btc_incoming_receiver,
			btc_outgoing_sender,
			state_chain_client
				.storage_value::<pallet_cf_vaults::CeremonyIdCounter<
					state_chain_runtime::Runtime,
					state_chain_runtime::BitcoinInstance,
				>>(state_chain_stream.cache().block_hash)
				.await
				.context("Failed to get Bitcoin CeremonyIdCounter from SC")?,
		);

	scope.spawn(btc_multisig_client_backend_future);

	witness::start::start(
		scope,
		&settings,
		state_chain_client.clone(),
		state_chain_stream.clone(),
		db.clone(),
	)
	.await?;

	scope.spawn(state_chain_observer::start(
		state_chain_client.clone(),
		state_chain_stream.clone(),
		EthBroadcaster::new(EthRpcClient::new(&settings.eth).await?),
		DotBroadcaster::new(DotHttpRpcClient::new(&settings.dot.http_node_endpoint).await?),
		BtcBroadcaster::new(btc_rpc_client.clone()),
		eth_multisig_client,
		dot_multisig_client,
		btc_multisig_client,
		peer_update_sender,
	));

	has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);
	Ok(())
}
