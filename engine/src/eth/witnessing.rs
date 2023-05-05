use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use cf_chains::{eth::assets, Ethereum};
use cf_primitives::Asset;
use futures::TryFutureExt;
use pallet_cf_environment::cfe;
use sp_core::{H160, H256};
use tokio::sync::Mutex;

use crate::{
	common::start_with_restart_on_failure,
	db::PersistentKeyDB,
	eth::deposit_witnesser::DepositWitnesser,
	settings,
	state_chain_observer::{client::StateChainClient, EthAddressToMonitorSender},
	witnesser::{AddressMonitor, EpochStart},
};
use utilities::task_scope::Scope;

use super::{
	chain_data_witnesser,
	contract_witnesser::ContractWitnesser,
	erc20_witnesser::Erc20Witnesser,
	eth_block_witnessing::{self},
	key_manager::KeyManager,
	rpc::EthDualRpcClient,
	state_chain_gateway::StateChainGateway,
	vault::Vault,
};
use itertools::Itertools;

use crate::state_chain_observer::client::storage_api::StorageApi;

use anyhow::Context;

pub struct AllWitnessers {
	pub key_manager: ContractWitnesser<KeyManager, StateChainClient>,
	pub state_chain_gateway: ContractWitnesser<StateChainGateway, StateChainClient>,
	pub vault: ContractWitnesser<Vault, StateChainClient>,
	pub eth_deposits: DepositWitnesser<StateChainClient>,
	pub flip_deposits: ContractWitnesser<Erc20Witnesser, StateChainClient>,
	pub usdc_deposits: ContractWitnesser<Erc20Witnesser, StateChainClient>,
}

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	eth_settings: &settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	expected_chain_id: web3::types::U256,
	initial_block_hash: H256,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	epoch_start_receiver_2: async_broadcast::Receiver<EpochStart<Ethereum>>,
	cfe_settings_update_receiver: tokio::sync::watch::Receiver<cfe::CfeSettings>,
	db: Arc<PersistentKeyDB>,
) -> anyhow::Result<EthAddressToMonitorSender> {
	scope.spawn(
		chain_data_witnesser::start(
			EthDualRpcClient::new(eth_settings, expected_chain_id).await.unwrap(),
			state_chain_client.clone(),
			epoch_start_receiver_2,
			cfe_settings_update_receiver,
		)
		.map_err(|_r| anyhow::anyhow!("eth::chain_data_witnesser::start failed")),
	);

	let state_chain_gateway_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumStateChainGatewayAddress<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to get StateChainGateway address from SC")?;

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let usdc_address = state_chain_client
		.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
			initial_block_hash,
			&Asset::Usdc,
		)
		.await
		.context("Failed to get USDC address from SC")?
		.expect("USDC address must exist at genesis");

	let flip_contract_address = state_chain_client
		.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
			initial_block_hash,
			&Asset::Flip,
		)
		.await
		.context("Failed to get FLIP address from SC")?
		.expect("FLIP address must exist at genesis");

	let eth_chain_deposit_addresses = state_chain_client
		.storage_map::<pallet_cf_ingress_egress::DepositAddressDetailsLookup<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>(initial_block_hash)
		.await
		.context("Failed to get initial ingress details")?
		.into_iter()
		.map(|(address, channel_details)| (channel_details.source_asset, address))
		.into_group_map();

	fn monitored_addresses_from_all_eth(
		eth_chain_deposit_addresses: &HashMap<assets::eth::Asset, Vec<H160>>,
		asset: assets::eth::Asset,
	) -> BTreeSet<H160> {
		if let Some(eth_deposit_addresses) = eth_chain_deposit_addresses.get(&asset) {
			eth_deposit_addresses.clone().into_iter().collect()
		} else {
			Default::default()
		}
	}

	let eth_addresses =
		monitored_addresses_from_all_eth(&eth_chain_deposit_addresses, assets::eth::Asset::Eth);

	let usdc_addresses =
		monitored_addresses_from_all_eth(&eth_chain_deposit_addresses, assets::eth::Asset::Usdc);

	let flip_addresses =
		monitored_addresses_from_all_eth(&eth_chain_deposit_addresses, assets::eth::Asset::Flip);

	let (eth_monitor_command_sender, eth_address_monitor) = AddressMonitor::new(eth_addresses);

	let (usdc_monitor_command_sender, usdc_address_monitor) = AddressMonitor::new(usdc_addresses);

	let (flip_monitor_command_sender, flip_address_monitor) = AddressMonitor::new(flip_addresses);

	let epoch_start_receiver = Arc::new(Mutex::new(epoch_start_receiver));
	let eth_address_monitor = Arc::new(Mutex::new(eth_address_monitor));
	let usdc_address_monitor = Arc::new(Mutex::new(usdc_address_monitor));
	let flip_address_monitor = Arc::new(Mutex::new(flip_address_monitor));

	let eth_settings = eth_settings.clone();

	let create_and_run_witnesser_futures = move || {
		let eth_settings = eth_settings.clone();
		let state_chain_client = state_chain_client.clone();
		let db = db.clone();
		let epoch_start_receiver = epoch_start_receiver.clone();

		let flip_address_monitor = flip_address_monitor.clone();
		let usdc_address_monitor = usdc_address_monitor.clone();
		let eth_address_monitor = eth_address_monitor.clone();

		async move {
			// We create a new RPC on each call to the future, since one common reason for
			// failure is that the WS connection has been dropped. This ensures that we create a
			// new client, and therefore create a new connection.
			let dual_rpc =
				EthDualRpcClient::new(&eth_settings, expected_chain_id).await.map_err(|err| {
					tracing::error!("Failed to create EthDualRpcClient: {:?}", err);
				})?;

			eth_block_witnessing::start(
				epoch_start_receiver,
				AllWitnessers {
					key_manager: ContractWitnesser::new(
						KeyManager::new(key_manager_address.into()),
						state_chain_client.clone(),
						dual_rpc.clone(),
						false,
					),
					state_chain_gateway: ContractWitnesser::new(
						StateChainGateway::new(state_chain_gateway_address.into()),
						state_chain_client.clone(),
						dual_rpc.clone(),
						true,
					),
					vault: ContractWitnesser::new(
						Vault::new(vault_address.into()),
						state_chain_client.clone(),
						dual_rpc.clone(),
						true,
					),
					eth_deposits: DepositWitnesser::new(
						state_chain_client.clone(),
						dual_rpc.clone(),
						eth_address_monitor,
					),
					flip_deposits: ContractWitnesser::new(
						Erc20Witnesser::new(
							flip_contract_address.into(),
							assets::eth::Asset::Flip,
							flip_address_monitor,
						),
						state_chain_client.clone(),
						dual_rpc.clone(),
						false,
					),
					usdc_deposits: ContractWitnesser::new(
						Erc20Witnesser::new(
							usdc_address.into(),
							assets::eth::Asset::Usdc,
							usdc_address_monitor,
						),
						state_chain_client.clone(),
						dual_rpc.clone(),
						false,
					),
				},
				dual_rpc,
				db,
			)
			.await
		}
	};

	scope.spawn(async move {
		start_with_restart_on_failure(create_and_run_witnesser_futures).await;
		Ok(())
	});

	Ok(EthAddressToMonitorSender {
		eth: eth_monitor_command_sender,
		flip: flip_monitor_command_sender,
		usdc: usdc_monitor_command_sender,
	})
}
