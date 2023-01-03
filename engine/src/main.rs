use std::sync::Arc;

use crate::multisig::{eth::EthSigning, polkadot::PolkadotSigning};
use anyhow::Context;

use cf_primitives::AccountRole;
use chainflip_engine::{
	eth::{
		self, build_broadcast_channel, key_manager::KeyManager, rpc::EthDualRpcClient,
		stake_manager::StakeManager, EthBroadcaster,
	},
	health::HealthChecker,
	logging,
	multisig::{self, client::key_store::KeyStore, PersistentKeyDB},
	p2p,
	settings::{CommandLineOptions, Settings},
	state_chain_observer::{
		self,
		client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi},
	},
	task_scope::task_scope,
};

use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use pallet_cf_validator::SemVer;
use sp_core::U256;

#[cfg(feature = "ibiza")]
use chainflip_engine::dot::{rpc::DotRpcClient, DotBroadcaster};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	use_chainflip_account_id_encoding();

	let settings = Settings::new(CommandLineOptions::parse()).context("Error reading settings")?;

	// Note: the greeting should only be printed in normal mode (i.e. not for short-lived commands
	// like `--version`), so we execute it only after the settings have been parsed.
	utilities::print_starting!();

	let root_logger = logging::utils::new_json_logger_with_tag_filter(
		settings.log.whitelist.clone(),
		settings.log.blacklist.clone(),
	);

	task_scope(|scope| {
		async {
			if let Some(health_check_settings) = &settings.health_check {
				scope.spawn(HealthChecker::new(health_check_settings, &root_logger).await?.run());
			}

			let (latest_block_hash, state_chain_block_stream, state_chain_client) =
				state_chain_observer::client::StateChainClient::new(
					scope,
					&settings.state_chain,
					AccountRole::Validator,
					true,
					&root_logger,
				)
				.await?;

			let eth_dual_rpc = EthDualRpcClient::new(
				&settings.eth,
				U256::from(
					state_chain_client
						.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
							latest_block_hash,
						)
						.await
						.context("Failed to get EthereumChainId from state chain")?,
				),
				&root_logger,
			)
			.await
			.context("Failed to create EthDualRpcClient")?;

			let eth_broadcaster =
				EthBroadcaster::new(&settings.eth, eth_dual_rpc.clone(), &root_logger)
					.context("Failed to create ETH broadcaster")?;

			#[cfg(feature = "ibiza")]
			let dot_rpc_client = DotRpcClient::new(&settings.dot.ws_node_endpoint)
				.await
				.context("Failed to create Polkadot Client")?;

			state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_validator::Call::cfe_version {
						new_version: SemVer {
							major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
							minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
							patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
						},
					},
					&root_logger,
				)
				.await
				.context("Failed to submit version to state chain")?;

			let (epoch_start_sender, [epoch_start_receiver]) = build_broadcast_channel(10);

			#[cfg(feature = "ibiza")]
			let (dot_epoch_start_sender, [dot_epoch_start_receiver_1]) = build_broadcast_channel(10);

			let cfe_settings = state_chain_client
				.storage_value::<pallet_cf_environment::CfeSettings<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get on chain CFE settings from SC")?;

			let (cfe_settings_update_sender, cfe_settings_update_receiver) =
				tokio::sync::watch::channel(cfe_settings);

			let stake_manager_address = state_chain_client
				.storage_value::<pallet_cf_environment::EthereumStakeManagerAddress<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get StakeManager address from SC")?;
			let stake_manager_contract = StakeManager::new(stake_manager_address.into());

			let key_manager_address = state_chain_client
				.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get KeyManager address from SC")?;

			let key_manager_contract = KeyManager::new(key_manager_address.into());

			let latest_ceremony_id = state_chain_client
				.storage_value::<pallet_cf_validator::CeremonyIdCounter<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get CeremonyIdCounter from SC")?;

			let db = Arc::new(
				PersistentKeyDB::new_and_migrate_to_latest(
					settings.signing.db_file.as_path(),
					Some(state_chain_client.get_genesis_hash()),
					&root_logger,
				)
				.context("Failed to open database")?,
			);

			let (
				eth_outgoing_sender,
				eth_incoming_receiver,
				dot_outgoing_sender,
				dot_incoming_receiver,
				peer_update_sender,
				p2p_fut,
			) = p2p::start(
				state_chain_client.clone(),
				settings.node_p2p,
				latest_block_hash,
				&root_logger,
			)
			.await
			.context("Failed to start p2p module")?;

			scope.spawn(p2p_fut);

			let (eth_multisig_client, eth_multisig_client_backend_future) =
				multisig::start_client::<EthSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db.clone()),
					eth_incoming_receiver,
					eth_outgoing_sender,
					latest_ceremony_id,
					&root_logger,
				);

			scope.spawn(eth_multisig_client_backend_future);

			let (dot_multisig_client, dot_multisig_client_backend_future) =
				multisig::start_client::<PolkadotSigning>(
					state_chain_client.account_id(),
					KeyStore::new(db),
					dot_incoming_receiver,
					dot_outgoing_sender,
					latest_ceremony_id,
					&root_logger,
				);

			scope.spawn(dot_multisig_client_backend_future);

			// Start eth witnessers
			scope.spawn(eth::contract_witnesser::start(
				stake_manager_contract,
				eth_dual_rpc.clone(),
				epoch_start_receiver.clone(),
				true,
				state_chain_client.clone(),
				&root_logger,
			));
			scope.spawn(eth::contract_witnesser::start(
				key_manager_contract,
				eth_dual_rpc.clone(),
				epoch_start_receiver.clone(),
				false,
				state_chain_client.clone(),
				&root_logger,
			));
			scope.spawn(eth::chain_data_witnesser::start(
				eth_dual_rpc.clone(),
				state_chain_client.clone(),
				#[cfg(feature = "ibiza")]
				epoch_start_receiver.clone(),
				#[cfg(not(feature = "ibiza"))]
				epoch_start_receiver,
				cfe_settings_update_receiver,
				&root_logger,
			));

			#[cfg(feature = "ibiza")]
			let (eth_monitor_ingress_sender, eth_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			#[cfg(feature = "ibiza")]
			let (eth_monitor_flip_ingress_sender, eth_monitor_flip_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			#[cfg(feature = "ibiza")]
			let (eth_monitor_usdc_ingress_sender, eth_monitor_usdc_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			#[cfg(feature = "ibiza")]
			let (dot_monitor_ingress_sender, dot_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();
			#[cfg(feature = "ibiza")]
			let (dot_monitor_signature_sender, dot_monitor_signature_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			// Start state chain components
			scope.spawn(state_chain_observer::start(
				state_chain_client.clone(),
				state_chain_block_stream,
				eth_broadcaster,
				#[cfg(feature = "ibiza")]
				DotBroadcaster::new(dot_rpc_client.clone()),
				eth_multisig_client,
				dot_multisig_client,
				peer_update_sender,
				epoch_start_sender,
				#[cfg(feature = "ibiza")]
				eth_monitor_ingress_sender,
				#[cfg(feature = "ibiza")]
				eth_monitor_flip_ingress_sender,
				#[cfg(feature = "ibiza")]
				eth_monitor_usdc_ingress_sender,
				#[cfg(feature = "ibiza")]
				dot_epoch_start_sender,
				#[cfg(feature = "ibiza")]
				dot_monitor_ingress_sender,
				#[cfg(feature = "ibiza")]
				dot_monitor_signature_sender,
				cfe_settings_update_sender,
				latest_block_hash,
				root_logger.clone(),
			));

			#[cfg(feature = "ibiza")]
			{
				use cf_primitives::{chains::assets, Asset};
				use chainflip_engine::{dot, eth::erc20_witnesser::Erc20Witnesser};
				use itertools::Itertools;
				use sp_core::H160;
				use std::collections::{BTreeSet, HashMap};

				let flip_contract_address = state_chain_client
					.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
						latest_block_hash,
						&Asset::Flip,
					)
					.await
					.context("Failed to get FLIP address from SC")?
					.expect("FLIP address must exist at genesis");

				let usdc_contract_address = state_chain_client
					.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
						latest_block_hash,
						&Asset::Usdc,
					)
					.await
					.context("Failed to get USDC address from SC")?
					.expect("USDC address must exist at genesis");

				let eth_chain_ingress_addresses = state_chain_client
					.storage_map::<pallet_cf_ingress_egress::IntentIngressDetails<
						state_chain_runtime::Runtime,
						state_chain_runtime::EthereumInstance,
					>>(latest_block_hash)
					.await
					.context("Failed to get initial ingress details")?
					.into_iter()
					.map(|(address, intent)| (intent.ingress_asset, address))
					.into_group_map();

				fn monitored_addresses_from_all_eth(
					eth_chain_ingress_addresses: &HashMap<assets::eth::Asset, Vec<H160>>,
					asset: assets::eth::Asset,
				) -> BTreeSet<H160> {
					if let Some(eth_ingress_addresses) = eth_chain_ingress_addresses.get(&asset) {
						eth_ingress_addresses.clone()
					} else {
						Default::default()
					}
					.iter()
					.cloned()
					.collect()
				}

				scope.spawn(eth::ingress_witnesser::start(
					eth_dual_rpc.clone(),
					epoch_start_receiver.clone(),
					eth_monitor_ingress_receiver,
					state_chain_client.clone(),
					monitored_addresses_from_all_eth(
						&eth_chain_ingress_addresses,
						assets::eth::Asset::Eth,
					),
					&root_logger,
				));
				scope.spawn(eth::contract_witnesser::start(
					Erc20Witnesser::new(
						flip_contract_address.into(),
						assets::eth::Asset::Flip,
						monitored_addresses_from_all_eth(
							&eth_chain_ingress_addresses,
							assets::eth::Asset::Flip,
						),
						eth_monitor_flip_ingress_receiver,
					),
					eth_dual_rpc.clone(),
					epoch_start_receiver.clone(),
					false,
					state_chain_client.clone(),
					&root_logger,
				));
				scope.spawn(eth::contract_witnesser::start(
					Erc20Witnesser::new(
						usdc_contract_address.into(),
						assets::eth::Asset::Usdc,
						monitored_addresses_from_all_eth(
							&eth_chain_ingress_addresses,
							assets::eth::Asset::Usdc,
						),
						eth_monitor_usdc_ingress_receiver,
					),
					eth_dual_rpc,
					epoch_start_receiver,
					false,
					state_chain_client.clone(),
					&root_logger,
				));
				scope.spawn(dot::witnesser::start(
					dot_epoch_start_receiver_1,
					dot_rpc_client,
					dot_monitor_ingress_receiver,
					state_chain_client
						.storage_map::<pallet_cf_ingress_egress::IntentIngressDetails<
							state_chain_runtime::Runtime,
							state_chain_runtime::PolkadotInstance,
						>>(latest_block_hash)
						.await
						.context("Failed to get initial ingress details")?
						.into_iter()
						.filter_map(|(address, intent)| {
							if intent.ingress_asset ==
								cf_primitives::chains::assets::dot::Asset::Dot
							{
								Some(address)
							} else {
								None
							}
						})
						.collect(),
					dot_monitor_signature_receiver,
					// NB: We don't need to monitor Ethereum signatures because we already monitor
					// siganture accepted events from the KeyManager contract on Ethereum.
					state_chain_client
						.storage_map::<pallet_cf_broadcast::SignatureToBroadcastIdLookup<
							state_chain_runtime::Runtime,
							state_chain_runtime::PolkadotInstance,
						>>(latest_block_hash)
						.await
						.context("Failed to get initial DOT signatures to monitor")?
						.into_iter()
						.map(|(signature, _)| signature.0)
						.collect(),
					state_chain_client,
					&root_logger,
				))
			}

			Ok(())
		}
		.boxed()
	})
	.await
}
