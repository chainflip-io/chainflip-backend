use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use cf_chains::{eth::assets, Ethereum};
use cf_primitives::Asset;
use futures::TryFutureExt;
use pallet_cf_environment::cfe;
use sp_core::{H160, H256};

use crate::{
	common::start_with_restart_on_failure,
	eth::ingress_witnesser::IngressWitnesser,
	multisig::PersistentKeyDB,
	settings,
	state_chain_observer::{client::StateChainClient, EthAddressToMonitorSender},
	task_scope::Scope,
	try_with_logging,
	witnesser::EpochStart,
};

use super::{
	chain_data_witnesser,
	contract_witnesser::ContractWitnesser,
	erc20_witnesser::Erc20Witnesser,
	eth_block_witnessing::{self, IngressAddressReceiverPairs},
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

	let stake_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumStakeManagerAddress<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to get StakeManager address from SC")?;

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			initial_block_hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let eth_chain_ingress_addresses = state_chain_client
		.storage_map::<pallet_cf_ingress_egress::IntentIngressDetails<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>(initial_block_hash)
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

	let usdc_address = state_chain_client
		.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
			initial_block_hash,
			&Asset::Usdc,
		)
		.await
		.context("Failed to get USDC address from SC")?
		.expect("USDC address must exist at genesis");

	let eth_addresses =
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Eth);

	let usdc_addresses =
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Usdc);

	let flip_contract_address = state_chain_client
		.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
			initial_block_hash,
			&Asset::Flip,
		)
		.await
		.context("Failed to get FLIP address from SC")?
		.expect("FLIP address must exist at genesis");

	let flip_addresses =
		monitored_addresses_from_all_eth(&eth_chain_ingress_addresses, assets::eth::Asset::Flip);

	let (eth_ingress_sender, eth_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (flip_ingress_sender, flip_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (usdc_ingress_sender, usdc_ingress_receiver) = tokio::sync::mpsc::unbounded_channel();

	let eth_settings = eth_settings.clone();
	let create_and_run_witnesser_futures = move |(epoch_start_receiver, ingress_receiver_pairs)| {
		let eth_settings = eth_settings.clone();
		let state_chain_client = state_chain_client.clone();
		let db = db.clone();
		async move {
			// We create a new RPC on each call to the future, since one common reason for
			// failure is that the WS connection has been dropped. This ensures that we create a
			// new client, and therefore create a new connection.
			let dual_rpc = try_with_logging!(
				EthDualRpcClient::new(&eth_settings, expected_chain_id).await,
				(epoch_start_receiver, ingress_receiver_pairs)
			);

			let IngressAddressReceiverPairs {
				eth: (eth_ingress_receiver, eth_addresses),
				flip: (flip_ingress_receiver, flip_addresses),
				usdc: (usdc_ingress_receiver, usdc_addresses),
			} = ingress_receiver_pairs;

			eth_block_witnessing::start(
				epoch_start_receiver,
				AllWitnessers {
					key_manager: ContractWitnesser::new(
						KeyManager::new(key_manager_address.into()),
						state_chain_client.clone(),
						dual_rpc.clone(),
						false,
					),
					stake_manager: ContractWitnesser::new(
						StakeManager::new(stake_manager_address.into()),
						state_chain_client.clone(),
						dual_rpc.clone(),
						true,
					),
					eth_ingress: IngressWitnesser::new(
						state_chain_client.clone(),
						dual_rpc.clone(),
						eth_addresses,
						eth_ingress_receiver,
					),
					flip_ingress: ContractWitnesser::new(
						Erc20Witnesser::new(
							flip_contract_address.into(),
							assets::eth::Asset::Flip,
							flip_addresses,
							flip_ingress_receiver,
						),
						state_chain_client.clone(),
						dual_rpc.clone(),
						false,
					),
					usdc_ingress: ContractWitnesser::new(
						Erc20Witnesser::new(
							usdc_address.into(),
							assets::eth::Asset::Usdc,
							usdc_addresses,
							usdc_ingress_receiver,
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

	scope.spawn(start_with_restart_on_failure(
		create_and_run_witnesser_futures,
		(
			epoch_start_receiver,
			IngressAddressReceiverPairs {
				eth: (eth_ingress_receiver, eth_addresses),
				flip: (flip_ingress_receiver, flip_addresses),
				usdc: (usdc_ingress_receiver, usdc_addresses),
			},
		),
	));

	Ok(EthAddressToMonitorSender {
		eth: eth_ingress_sender,
		flip: flip_ingress_sender,
		usdc: usdc_ingress_sender,
	})
}
