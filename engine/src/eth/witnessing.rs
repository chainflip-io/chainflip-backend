use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use cf_chains::{eth::assets, Ethereum};
use cf_primitives::Asset;
use futures::TryFutureExt;
use pallet_cf_environment::cfe;
use sp_core::H160;

use crate::{
	common::start_with_restart_on_failure, eth::ingress_witnesser::IngressWitnesser,
	multisig::PersistentKeyDB, settings, state_chain_observer::client::StateChainClient,
	task_scope::Scope, try_or_throw, witnesser::EpochStart,
};

use super::{
	chain_data_witnesser,
	contract_witnesser::ContractWitnesser,
	erc20_witnesser::Erc20Witnesser,
	eth_block_witnessing::{self, IngressAddressReceivers},
	key_manager::KeyManager,
	rpc::EthDualRpcClient,
	stake_manager::StakeManager,
};
use itertools::Itertools;

use crate::state_chain_observer::client::storage_api::StorageApi;

use anyhow::Context;

pub struct AllWitnessers {
	pub key_manager: ContractWitnesser<KeyManager, StateChainClient>,
	pub stake_manager: ContractWitnesser<StakeManager, StateChainClient>,
	pub eth_ingress: IngressWitnesser<StateChainClient>,
	pub flip_ingress: ContractWitnesser<Erc20Witnesser, StateChainClient>,
	pub usdc_ingress: ContractWitnesser<Erc20Witnesser, StateChainClient>,
}

async fn create_witnessers(
	state_chain_client: &Arc<StateChainClient>,
	eth_dual_rpc: &EthDualRpcClient,
	latest_block_hash: sp_core::H256,
	ingress_adddress_receivers: IngressAddressReceivers,
	logger: &slog::Logger,
) -> anyhow::Result<AllWitnessers> {
	let IngressAddressReceivers {
		eth: eth_address_receiver,
		flip: flip_address_receiver,
		usdc: usdc_address_receiver,
	} = ingress_adddress_receivers;

	let key_manager_witnesser = ContractWitnesser::new(
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
	);

	let stake_manager_witnesser = ContractWitnesser::new(
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
	);

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

	let flip_contract_witnesser = ContractWitnesser::new(
		Erc20Witnesser::new(
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
			monitored_addresses_from_all_eth(
				&eth_chain_ingress_addresses,
				assets::eth::Asset::Flip,
			),
			flip_address_receiver,
		),
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	);

	let usdc_contract_witnesser = ContractWitnesser::new(
		Erc20Witnesser::new(
			state_chain_client
				.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
					latest_block_hash,
					&Asset::Usdc,
				)
				.await
				.context("Failed to get USDC address from SC")?
				.expect("USDC address must exist at genesis")
				.into(),
			assets::eth::Asset::Usdc,
			monitored_addresses_from_all_eth(
				&eth_chain_ingress_addresses,
				assets::eth::Asset::Usdc,
			),
			usdc_address_receiver,
		),
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	);

	let ingress_witnesser = IngressWitnesser::new(
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Eth),
		eth_address_receiver,
		logger,
	);

	Ok(AllWitnessers {
		key_manager: key_manager_witnesser,
		stake_manager: stake_manager_witnesser,
		eth_ingress: ingress_witnesser,
		flip_ingress: flip_contract_witnesser,
		usdc_ingress: usdc_contract_witnesser,
	})
}

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	eth_settings: settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	expected_chain_id: web3::types::U256,
	latest_block_hash: sp_core::H256,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	epoch_start_receiver_2: async_broadcast::Receiver<EpochStart<Ethereum>>,
	ingress_address_receivers: IngressAddressReceivers,
	cfe_settings_update_receiver: tokio::sync::watch::Receiver<cfe::CfeSettings>,
	db: Arc<PersistentKeyDB>,
	logger: slog::Logger,
) -> anyhow::Result<()> {
	scope.spawn(
		chain_data_witnesser::start(
			EthDualRpcClient::new(&eth_settings, expected_chain_id, &logger).await.unwrap(),
			state_chain_client.clone(),
			epoch_start_receiver_2,
			cfe_settings_update_receiver,
			logger.clone(),
		)
		.map_err(|_r| anyhow::anyhow!("eth::chain_data_witnesser::start failed")),
	);

	let create_and_run_witnesser_futures = move |(
		epoch_start_receiver,
		ingress_address_receivers,
	)| {
		let eth_settings = eth_settings.clone();
		let logger = logger.clone();
		let state_chain_client = state_chain_client.clone();
		let db = db.clone();
		async move {
			// We create a new RPC on each call to the future, since one common reason for
			// failure is that the WS connection has been dropped. This ensures that we create a
			// new client, and therefore create a new connection.
			let dual_rpc = try_or_throw!(
				EthDualRpcClient::new(&eth_settings, expected_chain_id, &logger).await,
				(epoch_start_receiver, ingress_address_receivers),
				&logger
			);
			eth_block_witnessing::start(
				epoch_start_receiver,
				create_witnessers(
					&state_chain_client,
					&dual_rpc,
					latest_block_hash,
					ingress_address_receivers,
					&logger,
				)
				.await
				.expect("If we failed here, we cannot connect to the StateChain, so allowing us to restart from here doesn't make much sense."),
				dual_rpc,
				db,
				logger.clone(),
			)
			.await
		}
	};

	start_with_restart_on_failure(
		scope,
		create_and_run_witnesser_futures,
		(epoch_start_receiver, ingress_address_receivers),
	)
	.await?;

	Ok(())
}
