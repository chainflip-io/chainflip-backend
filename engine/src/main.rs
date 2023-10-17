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
			chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
		},
	},
	witness::{self, common::STATE_CHAIN_CONNECTION},
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use multisig::{self, bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};
use std::{
	path::PathBuf,
	sync::{atomic::AtomicBool, Arc},
	time::Duration,
};
use tracing::info;
use utilities::{metrics, task_scope::task_scope, CachedStream};

lazy_static::lazy_static! {
	static ref CFE_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

async fn ensure_cfe_version_record_up_to_date(settings: &Settings) -> anyhow::Result<()> {
	use subxt::{ext::sp_core::Pair, PolkadotConfig};
	// We use subxt because it is capable of dynamic decoding of values, which is important because
	// the SC Client might be incompatible with the current runtime version.
	let subxt_client =
		subxt::OnlineClient::<PolkadotConfig>::from_url(&settings.state_chain.ws_endpoint).await?;

	let signer = subxt::tx::PairSigner::new(subxt::ext::sp_core::sr25519::Pair::from_seed(
		&utilities::read_clean_and_decode_hex_str_file(
			&settings.state_chain.signing_key_file,
			"Signing Key",
			|str| {
				<[u8; 32]>::try_from(hex::decode(str)?).map_err(|e| {
					anyhow::anyhow!("Failed to decode signing key: Wrong length. {e:?}")
				})
			},
		)?,
	));

	let recorded_version = <SemVer as codec::Decode>::decode(
		&mut subxt_client
			.storage()
			.at_latest()
			.await?
			.fetch_or_default(&subxt::storage::dynamic(
				"Validator",
				"NodeCFEVersion",
				vec![signer.account_id()],
			))
			.await?
			.encoded(),
	)
	.map_err(|e| anyhow::anyhow!("Failed to decode recorded_version: {e:?}"))?;

	// Note that around CFE upgrade period, the less recent version might still be running (and
	// can even be *the* "active" instance), so it is important that it doesn't downgrade the
	// version record:
	if CFE_VERSION.is_more_recent_than(recorded_version) {
		info!("Updating CFE version record from {:?} to {:?}", recorded_version, *CFE_VERSION);

		subxt_client
			.tx()
			.sign_and_submit_then_watch_default(
				&subxt::dynamic::tx(
					"Validator",
					"cfe_version",
					vec![(
						"new_version",
						vec![
							("major", CFE_VERSION.major),
							("minor", CFE_VERSION.minor),
							("patch", CFE_VERSION.patch),
						],
					)],
				),
				&signer,
			)
			.await?
			.wait_for_in_block()
			.await?;
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
	utilities::print_starting!();

	task_scope(|scope| {
		async move {
			let mut start_logger_server_fn =
				Some(utilities::init_json_logger(settings.logging.span_lifecycle).await);

			ensure_cfe_version_record_up_to_date(&settings).await?;

			let has_completed_initialising = Arc::new(AtomicBool::new(false));

			let (state_chain_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Validator,
					true,
					Some(*CFE_VERSION),
					true,
				)
				.await?;

			// In case we are upgrading, this gives the old CFE more time to release system
			// resources.
			tokio::time::sleep(Duration::from_secs(3)).await;

			// Wait until SCC has started, to ensure old engine has stopped
			start_logger_server_fn.take().expect("only called once")(scope);

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
				health::start(scope, health_check_settings, has_completed_initialising.clone())
					.await?;
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
						.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<
							state_chain_runtime::Runtime,
						>>(state_chain_client.latest_finalized_hash())
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
		.boxed()
	})
	.await
}
