use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
	time::Duration,
};

use cf_chains::{eth::assets, Ethereum};
use cf_primitives::Asset;
use futures::Future;
use sp_core::H160;
use tokio::task::JoinHandle;

use crate::{
	eth::ingress_witnesser::IngressWitnesser, settings,
	state_chain_observer::client::StateChainClient, task_scope::Scope, witnesser::EpochStart,
};

use super::{
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

pub type AllWitnessers = (
	ContractWitnesser<KeyManager, StateChainClient>,
	ContractWitnesser<StakeManager, StateChainClient>,
	IngressWitnesser<StateChainClient>,
	ContractWitnesser<Erc20Witnesser, StateChainClient>,
	ContractWitnesser<Erc20Witnesser, StateChainClient>,
);

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
		flip_address_receiver,
	);

	let flip_contract_witnesser = ContractWitnesser::new(
		flip_witnesser,
		state_chain_client.clone(),
		eth_dual_rpc.clone(),
		false,
		logger,
	);

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
		usdc_address_receiver,
	);

	let usdc_contract_witnesser = ContractWitnesser::new(
		usdc_witnesser,
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

	Ok((
		key_manager_witnesser,
		stake_manager_witnesser,
		ingress_witnesser,
		flip_contract_witnesser,
		usdc_contract_witnesser,
	))
}

pub async fn start(
	scope: &Scope<'_, anyhow::Error>,
	eth_settings: settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	expected_chain_id: web3::types::U256,
	latest_block_hash: sp_core::H256,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	ingress_address_receivers: IngressAddressReceivers,
	logger: slog::Logger,
) -> anyhow::Result<()> {
	let create_and_run_witnesser_futures =
		move |(epoch_start_receiver, ingress_address_receivers)| {
			let eth_settings = eth_settings.clone();
			let logger = logger.clone();
			let state_chain_client = state_chain_client.clone();
			// let expected_chain_id = expected_chain_id.clone();
			async move {
				// We create a new RPC on each call to the future, since one common reason for
				// failure is that the WS connection has been dropped. This ensures that we create a
				// new client, and therefore create a new connection.
				let dual_rpc =
					EthDualRpcClient::new(&eth_settings, expected_chain_id, &logger).await.unwrap();
				let witnessers = create_witnessers(
					&state_chain_client,
					&dual_rpc,
					latest_block_hash,
					ingress_address_receivers,
					&logger,
				)
				.await
				.unwrap();
				eth_block_witnessing::start(
					epoch_start_receiver,
					dual_rpc,
					witnessers,
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
	.await
}

async fn start_with_restart_on_failure<StaticState, TaskFut, TaskGenerator>(
	scope: &Scope<'_, anyhow::Error>,
	task: TaskGenerator,
	static_state: StaticState,
) -> anyhow::Result<()>
where
	TaskFut: Future<Output = Result<(), StaticState>> + Send + 'static,
	StaticState: Send + 'static,
	TaskGenerator: Fn(StaticState) -> TaskFut + Send + 'static,
{
	scope.spawn(async move {

		let mut current_task: Option<JoinHandle<Result<(), StaticState>>> = None;
		let mut static_state = Some(static_state);

		loop {
			// Spawn with handle and then wait for future to finish
			if let Some(current_task) = current_task.take() {
				match current_task.await.unwrap() {
					Ok(()) => break Ok(()),
					Err(state) => {
						static_state = Some(state);
                        // give it some time before the next restart
                        tokio::time::sleep(Duration::from_secs(2)).await;
					},
				}
			}

			// TODO: Use scope when it can accept any errors, not just anyhow errors
			current_task = Some(tokio::spawn(task(static_state.take().expect("We only get here on error, where we set this. Or on first loop, where we set this."))));
		}
	});

	Ok(())
}

#[cfg(test)]
mod tests {
	use futures::FutureExt;

	use crate::task_scope::task_scope;

	use super::*;

	#[tokio::test(start_paused = true)]
	async fn test_restart_on_failure() {
		async fn start_up_some_loop(mut my_state: u32) -> Result<(), u32> {
			my_state += 1;
			let mut counter = 0;
			let end_number = 9;
			for i in my_state..end_number {
				counter += 1;

				// will take 4 restarts (i.e. my_state needs to be 5), since each counts from 0
				// before we get to 9
				if i == 4 {
					return Err(my_state)
				}
			}
			assert_eq!(my_state, 5);
			assert_eq!(counter, end_number - my_state);
			Ok(())
		}

		// Ensure the sleeps are run. Doesn't actually sleep for 100 seconds with `start_paused =
		// true`
		tokio::time::sleep(Duration::from_secs(100)).await;

		task_scope(|scope| {
			let value = 0;
			start_with_restart_on_failure(scope, start_up_some_loop, value).boxed()
		})
		.await
		.unwrap();
	}
}
