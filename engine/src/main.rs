use std::sync::Arc;

use anyhow::Context;

use cf_primitives::AccountRole;
use chainflip_engine::{
	btc::{self, rpc::BtcRpcClient, BtcBroadcaster},
	db::{KeyStore, PersistentKeyDB},
	dot::{self, rpc::DotRpcClient, witnesser as dot_witnesser, DotBroadcaster},
	eth::{self, build_broadcast_channel, rpc::EthHttpRpcClient, EthBroadcaster},
	health, p2p,
	settings::{CommandLineOptions, Settings},
	state_chain_observer::{
		self,
		client::{
			extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
			storage_api::StorageApi,
		},
	},
	witnesser::ItemMonitor,
};
use multisig::{self, bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};
use utilities::task_scope::task_scope;

use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::{FutureExt, TryFutureExt};
use pallet_cf_validator::SemVer;
use utilities::CachedStream;
use web3::types::U256;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	use_chainflip_account_id_encoding();

	let settings = Settings::new(CommandLineOptions::parse()).context("Error reading settings")?;

	// Note: the greeting should only be printed in normal mode (i.e. not for short-lived commands
	// like `--version`), so we execute it only after the settings have been parsed.
	utilities::print_starting!();

	task_scope(|scope| {
		async move {
			utilities::init_json_logger(scope).await;

			let (state_chain_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Validator,
					true,
				)
				.await?;

			let expected_chain_id = U256::from(
				state_chain_client
					.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
						state_chain_stream.cache().block_hash,
					)
					.await
					.context("Failed to get EthereumChainId from state chain")?,
			);

			let dot_rpc_client = DotRpcClient::new(&settings.dot.ws_node_endpoint)
				.await
				.context("Failed to create Polkadot Client")?;

			let btc_rpc_client =
				BtcRpcClient::new(&settings.btc).context("Failed to create Bitcoin Client")?;

			state_chain_client
				.submit_signed_extrinsic(pallet_cf_validator::Call::cfe_version {
					new_version: SemVer {
						major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
						minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
						patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
					},
				})
				.await
				.until_finalized()
				.await
				.context("Failed to submit version to state chain")?;

			let (epoch_start_sender, [epoch_start_receiver_1, epoch_start_receiver_2]) =
				build_broadcast_channel(10);

			let (dot_epoch_start_sender, [dot_epoch_start_receiver_1, dot_epoch_start_receiver_2]) =
				build_broadcast_channel(10);

			let (btc_epoch_start_sender, [btc_epoch_start_receiver]) = build_broadcast_channel(10);

			let cfe_settings = state_chain_client
				.storage_value::<pallet_cf_environment::CfeSettings<state_chain_runtime::Runtime>>(
					state_chain_stream.cache().block_hash,
				)
				.await
				.context("Failed to get on chain CFE settings from SC")?;

			let (cfe_settings_update_sender, cfe_settings_update_receiver) =
				tokio::sync::watch::channel(cfe_settings);

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
				settings.node_p2p,
				state_chain_stream.cache().block_hash,
			)
			.await
			.context("Failed to start p2p module")?;

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

			let (dot_monitor_signature_sender, dot_monitor_signature_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let eth_address_to_monitor = eth::witnessing::start(
				scope,
				&settings.eth,
				state_chain_client.clone(),
				expected_chain_id,
				state_chain_stream.cache().block_hash,
				epoch_start_receiver_1,
				epoch_start_receiver_2,
				cfe_settings_update_receiver,
				db.clone(),
			)
			.await
			.unwrap();

			let (btc_monitor_command_sender, btc_tx_hash_sender) = btc::witnessing::start(
				scope,
				state_chain_client.clone(),
				&settings.btc,
				btc_epoch_start_receiver,
				state_chain_stream.cache().block_hash,
				db.clone(),
			)
			.await
			.unwrap();

			let (dot_monitor_command_sender, dot_address_monitor) = ItemMonitor::new(
				state_chain_client
					.storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
						state_chain_runtime::Runtime,
						state_chain_runtime::PolkadotInstance,
					>>(state_chain_stream.cache().block_hash)
					.await
					.context("Failed to get initial deposit details")?
					.into_iter()
					.filter_map(|(address, channel_details)| {
						if channel_details.source_asset ==
							cf_primitives::chains::assets::dot::Asset::Dot
						{
							Some(address)
						} else {
							None
						}
					})
					.collect(),
			);

			scope.spawn(state_chain_observer::start(
				state_chain_client.clone(),
				state_chain_stream.clone(),
				EthBroadcaster::new(
					&settings.eth,
					EthHttpRpcClient::new(&settings.eth, Some(expected_chain_id))
						.await
						.context("Failed to create EthHttpRpcClient")?,
				)
				.context("Failed to create ETH broadcaster")?,
				DotBroadcaster::new(dot_rpc_client.clone()),
				BtcBroadcaster::new(btc_rpc_client.clone()),
				eth_multisig_client,
				dot_multisig_client,
				btc_multisig_client,
				peer_update_sender,
				epoch_start_sender,
				eth_address_to_monitor,
				dot_epoch_start_sender,
				dot_monitor_command_sender,
				dot_monitor_signature_sender,
				btc_epoch_start_sender,
				btc_monitor_command_sender,
				btc_tx_hash_sender,
				cfe_settings_update_sender,
			));

			scope.spawn(
				dot_witnesser::start(
					dot_epoch_start_receiver_1,
					dot_rpc_client.clone(),
					dot_address_monitor,
					dot_monitor_signature_receiver,
					// NB: We don't need to monitor Ethereum signatures because we already monitor
					// signature accepted events from the KeyManager contract on Ethereum.
					state_chain_client
						.storage_map::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
							state_chain_runtime::Runtime,
							state_chain_runtime::PolkadotInstance,
						>>(state_chain_stream.cache().block_hash)
						.await
						.context("Failed to get initial DOT signatures to monitor")?
						.into_iter()
						.map(|(signature, _)| signature)
						.collect(),
					state_chain_client.clone(),
					db,
				)
				.map_err(|_r| anyhow::anyhow!("DOT witnesser failed")),
			);

			scope.spawn(
				dot::runtime_version_updater::start(
					dot_epoch_start_receiver_2,
					dot_rpc_client,
					state_chain_client,
					state_chain_stream.cache().block_hash,
				)
				.map_err(|_| anyhow::anyhow!("DOT runtime version updater failed")),
			);

			if let Some(health_check_settings) = &settings.health_check {
				health::start(scope, health_check_settings).await?;
			}

			Ok(())
		}
		.boxed()
	})
	.await
}
