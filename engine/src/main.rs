use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use anyhow::Context;

use cf_primitives::{chains::assets, AccountRole, Asset};
use chainflip_engine::{
	dot::{rpc::DotRpcClient, witnesser as dot_witnesser, DotBroadcaster},
	eth::{
		self, build_broadcast_channel,
		contract_witnesser::ContractWitnesser,
		erc20_witnesser::Erc20Witnesser,
		eth_block_witnessing::{self, BlockProcessor},
		key_manager::KeyManager,
		rpc::EthDualRpcClient,
		stake_manager::StakeManager,
		EthBroadcaster,
	},
	health::HealthChecker,
	logging,
	multisig::{
		self, client::key_store::KeyStore, eth::EthSigning, polkadot::PolkadotSigning,
		PersistentKeyDB,
	},
	p2p,
	settings::{CommandLineOptions, Settings},
	state_chain_observer::{
		self,
		client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi, StateChainClient},
		EthAddressToMonitorSender,
	},
	task_scope::task_scope,
};
use eth::ingress_witnesser::IngressWitnesser;
use itertools::Itertools;
use sp_core::H160;

use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use clap::Parser;
use futures::FutureExt;
use pallet_cf_validator::SemVer;
use web3::types::U256;

async fn create_witnessers(
	state_chain_client: &Arc<StateChainClient>,
	eth_dual_rpc: &EthDualRpcClient,
	latest_block_hash: sp_core::H256,
	logger: &slog::Logger,
) -> anyhow::Result<([Box<dyn BlockProcessor>; 5], EthAddressToMonitorSender)> {
	let (eth_monitor_ingress_sender, eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let key_manager_witnesser = Box::new(ContractWitnesser::new(
		KeyManager::new(
			state_chain_client
				.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get KeyManager address from SC")?
				.into(),
		),
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	));

	let stake_manager_witnesser = Box::new(ContractWitnesser::new(
		StakeManager::new(
			state_chain_client
				.storage_value::<pallet_cf_environment::EthereumStakeManagerAddress<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get StakeManager address from SC")?
				.into(),
		),
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		true,
		logger,
	));

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

	let flip_witnesser = Erc20Witnesser::new(
		state_chain_client
			.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
				latest_block_hash,
				&Asset::Flip,
			)
			.await
			.context("Failed to get FLIP address from SC")?
			.expect("FLIP address must exist at genesis")
			.into(),
		assets::eth::Asset::Flip,
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Flip),
		eth_monitor_flip_ingress_receiver,
	);

	let flip_contract_witnesser = Box::new(ContractWitnesser::new(
		flip_witnesser,
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	));

	let usdc_contract_address = state_chain_client
		.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
			latest_block_hash,
			&Asset::Usdc,
		)
		.await
		.context("Failed to get USDC address from SC")?
		.expect("USDC address must exist at genesis");

	let usdc_witnesser = Erc20Witnesser::new(
		usdc_contract_address.into(),
		assets::eth::Asset::Usdc,
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Usdc),
		eth_monitor_usdc_ingress_receiver,
	);

	let usdc_contract_witnesser = Box::new(ContractWitnesser::new(
		usdc_witnesser,
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	));

	let ingress_witnesser = Box::new(IngressWitnesser::new(
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Eth),
		eth_monitor_ingress_receiver,
		logger,
	));

	Ok((
		[
			key_manager_witnesser,
			stake_manager_witnesser,
			flip_contract_witnesser,
			usdc_contract_witnesser,
			ingress_witnesser,
		],
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
	))
}

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

			let (epoch_start_sender, [epoch_start_receiver_1, epoch_start_receiver_2]) =
				build_broadcast_channel(10);

			let (dot_epoch_start_sender, [dot_epoch_start_receiver]) = build_broadcast_channel(10);

			let cfe_settings = state_chain_client
				.storage_value::<pallet_cf_environment::CfeSettings<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.context("Failed to get on chain CFE settings from SC")?;

			let (cfe_settings_update_sender, cfe_settings_update_receiver) =
				tokio::sync::watch::channel(cfe_settings);

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
					KeyStore::new(db.clone()),
					dot_incoming_receiver,
					dot_outgoing_sender,
					latest_ceremony_id,
					&root_logger,
				);

			scope.spawn(dot_multisig_client_backend_future);

			// Spawn Ethereum "block" witnessers
			let (eth_block_witnessers, eth_address_to_monitor_sender) = create_witnessers(
				&state_chain_client,
				&eth_dual_rpc,
				latest_block_hash,
				&root_logger,
			)
			.await?;

			scope.spawn(eth_block_witnessing::start(
				epoch_start_receiver_1,
				eth_dual_rpc.clone(),
				eth_block_witnessers,
				db,
				&root_logger,
			));

			// This witnesser is spawned separately because it does
			// not use eth block subscription
			scope.spawn(eth::chain_data_witnesser::start(
				eth_dual_rpc.clone(),
				state_chain_client.clone(),
				epoch_start_receiver_2,
				cfe_settings_update_receiver,
				&root_logger,
			));

			let (dot_monitor_ingress_sender, dot_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (dot_monitor_signature_sender, dot_monitor_signature_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			// Start state chain components
			scope.spawn(state_chain_observer::start(
				state_chain_client.clone(),
				state_chain_block_stream,
				eth_broadcaster,
				DotBroadcaster::new(dot_rpc_client.clone()),
				eth_multisig_client,
				dot_multisig_client,
				peer_update_sender,
				epoch_start_sender,
				eth_address_to_monitor_sender,
				dot_epoch_start_sender,
				dot_monitor_ingress_sender,
				dot_monitor_signature_sender,
				cfe_settings_update_sender,
				latest_block_hash,
				root_logger.clone(),
			));

			scope.spawn(dot_witnesser::start(
				dot_epoch_start_receiver,
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
						if intent.ingress_asset == cf_primitives::chains::assets::dot::Asset::Dot {
							Some(address)
						} else {
							None
						}
					})
					.collect(),
				dot_monitor_signature_receiver,
				// NB: We don't need to monitor Ethereum signatures because we already monitor
				// signature accepted events from the KeyManager contract on Ethereum.
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
			));

			Ok(())
		}
		.boxed()
	})
	.await
}
