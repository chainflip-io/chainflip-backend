mod btc;
mod dot;
mod eth;
pub mod state_chain;

use self::state_chain::handle_call;
use crate::{settings::DepositTrackerSettings, store::RedisStore};
use cf_utilities::task_scope;
use chainflip_api::primitives::{
	chains::assets::eth::Asset as EthAsset, Asset, NetworkEnvironment,
};
use chainflip_engine::{
	state_chain_observer::{
		self,
		client::{
			chain_api::ChainApi, storage_api::StorageApi, StateChainClient, STATE_CHAIN_CONNECTION,
		},
	},
	witness::common::epoch_source::EpochSource,
};

use anyhow::anyhow;
use chainflip_engine::dot::PolkadotHash;
use sp_core::H160;
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct EnvironmentParameters {
	eth_chain_id: u64,
	eth_vault_address: H160,
	eth_address_checker_address: H160,
	flip_contract_address: H160,
	usdc_contract_address: H160,
	usdt_contract_address: H160,
	supported_erc20_tokens: HashMap<H160, Asset>,
	dot_genesis_hash: PolkadotHash,
	pub chainflip_network: NetworkEnvironment,
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
		*supported_erc20_tokens.get(&EthAsset::Flip).expect("FLIP not supported");

	let usdc_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdc).expect("USDC not supported");

	let usdt_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdt).expect("USDT not supported");

	let supported_erc20_tokens: HashMap<H160, Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset.into()))
		.collect();

	let dot_genesis_hash = PolkadotHash::from_slice(
		state_chain_client
			.storage_value::<pallet_cf_environment::PolkadotGenesisHash<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_block().hash,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION)
			.as_bytes(),
	);

	let chainflip_network = state_chain_client
		.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	EnvironmentParameters {
		eth_chain_id,
		eth_vault_address,
		flip_contract_address,
		usdc_contract_address,
		usdt_contract_address,
		eth_address_checker_address,
		supported_erc20_tokens,
		dot_genesis_hash,
		chainflip_network,
	}
}

pub(super) async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
	store: RedisStore,
) -> anyhow::Result<()> {
	let (state_chain_stream, unfinalized_chain_stream, state_chain_client) = {
		state_chain_observer::client::StateChainClient::<(), _>::connect_without_account(
			scope,
			&settings.state_chain_ws_endpoint,
		)
		.await?
	};

	let env_params = get_env_parameters(&state_chain_client).await;
	let chainflip_network = env_params.chainflip_network;

	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone()).await;

	let witness_call = {
		let state_chain_client = state_chain_client.clone();
		move |call: state_chain_runtime::RuntimeCall, _epoch_index| {
			let mut store = store.clone();
			let state_chain_client = state_chain_client.clone();

			async move {
				handle_call(call, &mut store, chainflip_network, state_chain_client)
					.await
					.map_err(|err| anyhow!("failed to handle call: {err:?}"))
					.unwrap()
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
	.await
}
