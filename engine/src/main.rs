use anyhow::Context;
use cf_chains::dot::PolkadotHash;
use cf_primitives::{AccountRole, SemVer};
use chainflip_engine::{
	btc::retry_rpc::BtcRetryRpcClient,
	db::{KeyStore, PersistentKeyDB},
	dot::retry_rpc::DotRetryRpcClient,
	eth::{retry_rpc::EthRetryRpcClient, rpc::EthRpcSigningClient},
	health, p2p,
	settings::{CommandLineOptions, Settings, DEFAULT_SETTINGS_DIR},
	state_chain_observer::{
		self,
		client::{
			chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi,
			storage_api::StorageApi, STATE_CHAIN_CONNECTION,
		},
	},
	witness,
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use multisig::{self, bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};
use std::{
	sync::{atomic::AtomicBool, Arc},
	time::Duration,
};
use utilities::{metrics, task_scope::task_scope, CachedStream};

lazy_static::lazy_static! {
	static ref CFE_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	use_chainflip_account_id_encoding();

	let opts = CommandLineOptions::parse();

	// the settings directory from opts.config_root that we'll use to read the settings file
	let settings = Settings::new_with_settings_dir(DEFAULT_SETTINGS_DIR, opts)
		.context("Error reading settings")?;

	// Note: the greeting should only be printed in normal mode (i.e. not for short-lived commands
	// like `--version`), so we execute it only after the settings have been parsed.
	utilities::print_start_and_end!(async run_main(settings));

	Ok(())
}

async fn run_main(settings: Settings) -> anyhow::Result<()> {
	task_scope(|scope| {
		async move {
			let mut start_logger_server_fn =
				Some(utilities::logging::init_json_logger(settings.logging.clone()).await);

			let has_completed_initialising = Arc::new(AtomicBool::new(false));

			let (state_chain_stream, unfinalised_state_chain_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Validator,
					true,
					Some((*CFE_VERSION, true)),
				)
				.await?;

			// In case we are upgrading, this gives the old CFE more time to release system
			// resources.
			tokio::time::sleep(Duration::from_secs(3)).await;

			// Wait until SCC has started, to ensure old engine has stopped
			start_logger_server_fn.take().expect("only called once")(scope);

			if let Some(health_check_settings) = &settings.health_check {
				health::start(scope, health_check_settings, has_completed_initialising.clone())
					.await?;
			}

			if let Some(prometheus_settings) = &settings.prometheus {
				metrics::start(scope, prometheus_settings).await?;
			}

			let db = Arc::new(
				PersistentKeyDB::open_and_migrate_to_latest(
					&settings.signing.db_file,
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
				p2p_ready_receiver,
				p2p_fut,
			) = p2p::start(
				state_chain_client.clone(),
				settings.node_p2p.clone(),
				state_chain_stream.cache().hash,
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
						>>(state_chain_stream.cache().hash)
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
						>>(state_chain_stream.cache().hash)
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
						>>(state_chain_stream.cache().hash)
						.await
						.context("Failed to get Bitcoin CeremonyIdCounter from SC")?,
				);

			scope.spawn(btc_multisig_client_backend_future);

			// Create all the clients
			let eth_client = {
				let expected_eth_chain_id = web3::types::U256::from(
					state_chain_client
						.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION),
				);
				EthRetryRpcClient::<EthRpcSigningClient>::new(
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
						>>(state_chain_client.latest_finalized_block().hash)
						.await
						.expect(STATE_CHAIN_CONNECTION),
				);
				BtcRetryRpcClient::new(scope, settings.btc.nodes, expected_btc_network).await?
			};
			let dot_client = {
				let expected_dot_genesis_hash = PolkadotHash::from(
					state_chain_client
						.storage_value::<pallet_cf_environment::PolkadotGenesisHash<state_chain_runtime::Runtime>>(
							state_chain_client.latest_finalized_block().hash,
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
				unfinalised_state_chain_stream.clone(),
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

			p2p_ready_receiver.await.unwrap();

			has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

			Ok(())
		}
		.boxed()
	})
	.await
}
