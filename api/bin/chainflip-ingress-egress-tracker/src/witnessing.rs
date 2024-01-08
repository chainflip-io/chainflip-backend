mod btc;
pub mod btc_mempool;
mod dot;
mod eth;

use std::{collections::HashMap, sync::Arc};

use cf_chains::dot::PolkadotHash;
use cf_primitives::chains::assets::eth::Asset;
use chainflip_engine::{
	state_chain_observer::{
		self,
		client::{chain_api::ChainApi, storage_api::StorageApi, StateChainClient},
	},
	witness::common::{epoch_source::EpochSource, STATE_CHAIN_CONNECTION},
};
use sp_core::H160;
use utilities::task_scope;

use crate::DepositTrackerSettings;

#[derive(Clone)]
pub(super) struct EnvironmentParameters {
	eth_chain_id: u64,
	eth_vault_address: H160,
	eth_address_checker_address: H160,
	flip_contract_address: H160,
	usdc_contract_address: H160,
	supported_erc20_tokens: HashMap<H160, cf_primitives::Asset>,
	dot_genesis_hash: PolkadotHash,
	pub btc_network: cf_chains::btc::BitcoinNetwork,
}

async fn get_env_parameters(state_chain_client: &StateChainClient<()>) -> EnvironmentParameters {
	use state_chain_runtime::Runtime;

	let eth_chain_id = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumChainId<Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("State Chain client connection failed");

	let eth_vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("Failed to get Vault contract address from SC");

	let eth_address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumAddressCheckerAddress<Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("State Chain client connection failed");

	let supported_erc20_tokens: HashMap<_, _> = state_chain_client
		.storage_map::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("Failed to fetch Ethereum supported assets");

	let flip_contract_address =
		*supported_erc20_tokens.get(&Asset::Flip).expect("FLIP not supported");

	let usdc_contract_address =
		*supported_erc20_tokens.get(&Asset::Usdc).expect("USDC not supported");

	let supported_erc20_tokens: HashMap<H160, cf_primitives::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset.into()))
		.collect();

	let dot_genesis_hash = state_chain_client
		.storage_value::<pallet_cf_environment::PolkadotGenesisHash<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let btc_network = state_chain_client
		.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION)
		.into();

	EnvironmentParameters {
		eth_chain_id,
		eth_vault_address,
		flip_contract_address,
		usdc_contract_address,
		eth_address_checker_address,
		supported_erc20_tokens,
		dot_genesis_hash,
		btc_network,
	}
}

pub(super) async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
	witness_sender: tokio::sync::broadcast::Sender<state_chain_runtime::RuntimeCall>,
) -> anyhow::Result<(Arc<StateChainClient<()>>, EnvironmentParameters)> {
	let (state_chain_stream, unfinalized_chain_stream, state_chain_client) = {
		state_chain_observer::client::StateChainClient::connect_without_account(
			scope,
			&settings.state_chain_ws_endpoint,
			None,
		)
		.await?
	};

	let env_params = get_env_parameters(&state_chain_client).await;

	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone()).await;

	let witness_call = {
		let witness_sender = witness_sender.clone();
		move |call: state_chain_runtime::RuntimeCall, _epoch_index| {
			let witness_sender = witness_sender.clone();
			async move {
				// Send may fail if there aren't any subscribers,
				// but it is safe to ignore the error.
				if let Ok(n) = witness_sender.send(call.clone()) {
					tracing::info!("Broadcasting witnesser call {:?} to {} subscribers", call, n);
				}
			}
		}
	};

	eth::start(
		scope,
		state_chain_client.clone(),
		unfinalized_chain_stream.clone(),
		settings.clone(),
		env_params.clone(),
		epoch_source.clone(),
		witness_call.clone(),
	)
	.await?;

	btc::start(
		scope,
		witness_call.clone(),
		settings.clone(),
		env_params.clone(),
		state_chain_client.clone(),
		unfinalized_chain_stream.clone(),
		epoch_source.clone(),
	)
	.await?;

	dot::start(
		scope,
		witness_call,
		settings,
		env_params.clone(),
		state_chain_client.clone(),
		unfinalized_chain_stream,
		epoch_source,
	)
	.await?;

	Ok((state_chain_client, env_params))
}
