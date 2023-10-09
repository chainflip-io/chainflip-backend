use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use cf_primitives::chains::assets::eth::Asset;
use futures::FutureExt;
use utilities::task_scope::{self, task_scope};

use sp_core::H160;

use chainflip_engine::{
	eth::retry_rpc::EthersRetryRpcClient,
	settings::NodeContainer,
	state_chain_observer::{
		self,
		client::{chain_api::ChainApi, storage_api::StorageApi, StateChainStreamApi},
	},
	witness::{
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSource},
		eth::{
			erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents},
			EthSource,
		},
	},
};

use crate::DepositTrackerSettings;

async fn start_eth_witnessing(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	state_chain_client: Arc<state_chain_observer::client::StateChainClient<()>>,
	state_chain_stream: impl StateChainStreamApi + Clone,
	settings: DepositTrackerSettings,
	event_sender: tokio::sync::broadcast::Sender<state_chain_runtime::RuntimeCall>,
) -> anyhow::Result<()> {
	let eth_client = {
		let nodes = NodeContainer { primary: settings.eth_node.clone(), backup: None };

		let expected_eth_chain_id = state_chain_client
			.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.expect("State Chain client connection failed");

		EthersRetryRpcClient::new(
			scope,
			settings.eth_key_path,
			nodes,
			expected_eth_chain_id.into(),
		)?
	};
	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone()).await;

	let vaults = epoch_source.vaults().await;
	let eth_source = EthSource::new(eth_client.clone())
		.strictly_monotonic()
		.shared(scope)
		.chunk_by_vault(vaults);

	let eth_source_deposit_addresses = eth_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	let supported_erc20_tokens: HashMap<Asset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to fetch Ethereum supported assets")?;

	let flip_contract_address =
		*supported_erc20_tokens.get(&Asset::Flip).context("FLIP not supported")?;

	let usdc_contract_address =
		*supported_erc20_tokens.get(&Asset::Usdc).context("USDC not supported")?;

	let supported_erc20_tokens: HashMap<H160, cf_primitives::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset.into()))
		.collect();

	let witness_call = {
		let event_sender = event_sender.clone();
		move |call: state_chain_runtime::RuntimeCall, _epoch_index| {
			let event_sender = event_sender.clone();
			async move {
				event_sender.send(call).unwrap();
			}
		}
	};

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			witness_call.clone(),
			eth_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Usdc,
			usdc_contract_address,
		)
		.await?
		.logging("witnessing USDCDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, FlipEvents>(
			witness_call.clone(),
			eth_client.clone(),
			Asset::Flip,
			flip_contract_address,
		)
		.await?
		.logging("witnessing FlipDeposits")
		.spawn(scope);

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let state_chain_gateway_address = state_chain_client
        .storage_value::<pallet_cf_environment::EthereumStateChainGatewayAddress<state_chain_runtime::Runtime>>(
            state_chain_client.latest_finalized_hash(),
        )
        .await
        .context("Failed to get StateChainGateway address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.expect("State Chain client connection failed");

	eth_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			witness_call.clone(),
			eth_client.clone(),
			Asset::Eth,
			address_checker_address,
			vault_address,
		)
		.await
		.logging("witnessing EthereumDeposits")
		.spawn(scope);

	eth_source
		.clone()
		.vault_witnessing(
			witness_call.clone(),
			eth_client.clone(),
			vault_address,
			cf_primitives::Asset::Eth,
			cf_primitives::ForeignChain::Ethereum,
			supported_erc20_tokens.clone(),
		)
		.logging("witnessing Vault")
		.spawn(scope);

	eth_source
		.state_chain_gateway_witnessing(
			witness_call.clone(),
			eth_client,
			state_chain_gateway_address,
		)
		.logging("StateChainGateway")
		.spawn(scope);

	Ok(())
}

pub(super) fn start_witnesser(
	settings: DepositTrackerSettings,
	event_sender: tokio::sync::broadcast::Sender<state_chain_runtime::RuntimeCall>,
) {
	tokio::spawn(async move {
		task_scope(|scope| {
			async move {
				let (state_chain_stream, state_chain_client) = {
					state_chain_observer::client::StateChainClient::connect_without_account(
						scope,
						&settings.state_chain_ws_endpoint,
					)
					.await?
				};

				start_eth_witnessing(
					scope,
					state_chain_client.clone(),
					state_chain_stream.clone(),
					settings,
					event_sender.clone(),
				)
				.await
			}
			.boxed()
		})
		.await
		.unwrap();
	});
}
