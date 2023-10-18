use anyhow::Context;
use cf_chains::dot::PolkadotHash;
use cf_primitives::{AccountRole, SemVer};
use chainflip_engine::{
	btc::retry_rpc::BtcRetryRpcClient,
	db::{KeyStore, PersistentKeyDB},
	dot::retry_rpc::DotRetryRpcClient,
	eth::retry_rpc::EthersRetryRpcClient,
	health, p2p,
	settings::{CommandLineOptions, Settings, DEFAULT_SETTINGS_DIR},
	settings_migrate::migrate_settings0_9_3_to_0_10_0,
	state_chain_observer::{
		self,
		client::{
			chain_api::ChainApi,
			extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
			storage_api::StorageApi,
			StateChainClient, StateChainStreamApi,
		},
	},
	witness::{self, common::STATE_CHAIN_CONNECTION},
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use jsonrpsee::core::client::ClientT;
use multisig::{self, bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};
use std::{
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
};
use tracing::info;
use utilities::{
	make_periodic_tick, metrics,
	task_scope::{self, task_scope, ScopedJoinHandle},
};

lazy_static::lazy_static! {
	static ref CFE_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

enum CfeStatus {
	Active(ScopedJoinHandle<()>),
	Idle,
}

async fn ensure_cfe_version_record_up_to_date(
	state_chain_client: &Arc<StateChainClient>,
	state_chain_stream: &impl StateChainStreamApi,
) -> anyhow::Result<()> {
	let recorded_version = state_chain_client
		.storage_map_entry::<pallet_cf_validator::NodeCFEVersion<state_chain_runtime::Runtime>>(
			state_chain_stream.cache().block_hash,
			&state_chain_client.account_id(),
		)
		.await?;

	// Note that around CFE upgrade period, the less recent version might still be running (and
	// can even be *the* "active" instance), so it is important that it doesn't downgrade the
	// version record:
	if CFE_VERSION.is_more_recent_than(recorded_version) {
		info!("Updating CFE version record from {:?} to {:?}", recorded_version, *CFE_VERSION);

		state_chain_client
			.finalize_signed_extrinsic(pallet_cf_validator::Call::cfe_version {
				new_version: *CFE_VERSION,
			})
			.await
			.until_finalized()
			.await
			.context("Failed to submit version to state chain")?;
	}

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	use_chainflip_account_id_encoding();

	let opts = CommandLineOptions::parse();

	let config_root_path = PathBuf::from(&opts.config_root);

	// the settings directory from opts.config_root that we'll use to read the settings file
	let migrated_settings_dir = migrate_settings0_9_3_to_0_10_0(opts.config_root.clone())?;

	let settings = Settings::new_with_settings_dir(
		migrated_settings_dir.unwrap_or(DEFAULT_SETTINGS_DIR),
		opts,
	)
	.context("Error reading settings")?;

	// Note: the greeting should only be printed in normal mode (i.e. not for short-lived commands
	// like `--version`), so we execute it only after the settings have been parsed.
	utilities::print_start_and_end!(async run_main(settings, config_root_path, migrated_settings_dir));

	Ok(())
}

async fn run_main(
	settings: Settings,
	config_root_path: PathBuf,
	migrated_settings_dir: Option<&str>,
	task_scope(|scope| {
		async move {
			let mut start_logger_server_fn = Some(utilities::logging::init_json_logger(settings.logging.clone()).await);

			let ws_rpc_client = jsonrpsee::ws_client::WsClientBuilder::default()
				.build(&settings.state_chain.ws_endpoint)
				.await?;

			let mut cfe_status = CfeStatus::Idle;

			let (state_chain_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Validator,
					true,
				)
				.await?;


			if *CFE_VERSION == (SemVer { major: 0, minor: 10, patch: 0 })
			{
				if let Some(migrated_settings_dir) = migrated_settings_dir {
					// Back up the old settings.
					std::fs::copy(
						config_root_path.join(DEFAULT_SETTINGS_DIR).join("Settings.toml"),
						config_root_path.join(DEFAULT_SETTINGS_DIR).join("Settings.backup-0.9.3.toml"),
					)
					.context("Unable to back up old settings. Please ensure the chainflip-engine has write permissions in the config directories.")?;

					// Replace with the migrated settings.
					std::fs::rename(
						config_root_path.join(migrated_settings_dir).join("Settings.toml"),
						config_root_path.join(DEFAULT_SETTINGS_DIR).join("Settings.toml"),
					)
					.context("Unable to replace old settings with migrated settings. Please ensure the chainflip-engine has write permissions in the config directories.")?;

					// Remove the migration dir.
					std::fs::remove_dir_all(config_root_path.join(migrated_settings_dir))
						.unwrap_or_else(|e| {
							tracing::warn!(
								"Unable to remove migration dir: {e:?}. Please remove it manually.",
								e = e
							)
						});
				}
			}

	if let Some(health_check_settings) = &settings.health_check {
		health::start(scope, health_check_settings, has_completed_initialising.clone()).await?;
	}

	if let Some(prometheus_settings) = &settings.prometheus {
		metrics::start(scope, prometheus_settings).await?;
	}

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

	// Create all the clients
	let eth_client = {
		let expected_eth_chain_id = web3::types::U256::from(
			state_chain_client
				.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
					state_chain_client.latest_finalized_hash(),
				)
				.await
				.expect(STATE_CHAIN_CONNECTION),
		);
		EthersRetryRpcClient::new(
			scope,
			settings.eth.private_key_file,
			settings.eth.nodes,
			expected_eth_chain_id,
		)?
	};
	let btc_client = {
		let expected_btc_network = cf_chains::btc::BitcoinNetwork::from(
			state_chain_client
				.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<state_chain_runtime::Runtime>>(
					state_chain_client.latest_finalized_hash(),
				)
				.await
				.expect(STATE_CHAIN_CONNECTION),
		);
		BtcRetryRpcClient::new(scope, settings.btc.nodes, expected_btc_network).await?
	};
	let dot_client = {
		let expected_dot_genesis_hash = PolkadotHash::from(
			state_chain_client
				.storage_value::<pallet_cf_environment::PolkadotGenesisHash<state_chain_runtime::Runtime>>(
					state_chain_client.latest_finalized_hash(),
				)
				.await
				.expect(STATE_CHAIN_CONNECTION),
		);
		DotRetryRpcClient::new(scope, settings.dot.nodes, expected_dot_genesis_hash)?
	};

	witness::start::start(
		scope,
		eth_client.clone(),
		btc_client.clone(),
		dot_client.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		db.clone(),
	)
	.await?;

	scope.spawn(state_chain_observer::start(
		state_chain_client.clone(),
		state_chain_stream.clone(),
		eth_client,
		dot_client,
		btc_client,
		eth_multisig_client,
		dot_multisig_client,
		btc_multisig_client,
		peer_update_sender,
	));

	has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

	Ok(())
}
