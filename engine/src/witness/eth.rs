use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	eth::{
		retry_rpc::EthersRetryRpcClient,
		rpc::{EthRpcClient, ReconnectSubscriptionClient},
	},
	settings,
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
		StateChainStreamApi,
	},
	witness::erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents},
};

use super::{
	chain_source::{eth_source::EthSource, extension::ChainSourceExt},
	common::STATE_CHAIN_CONNECTION,
	epoch_source::EpochSourceBuilder,
	vault::EthAssetApi,
};

use anyhow::{Context, Result};

const SAFETY_MARGIN: usize = 7;

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient:
		StorageApi + ChainApi + EthAssetApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone,
{
	let expected_chain_id = web3::types::U256::from(
		state_chain_client
			.storage_value::<pallet_cf_environment::EthereumChainId<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.expect(STATE_CHAIN_CONNECTION),
	);

	let state_chain_gateway_address = state_chain_client
        .storage_value::<pallet_cf_environment::EthereumStateChainGatewayAddress<state_chain_runtime::Runtime>>(
            state_chain_client.latest_finalized_hash(),
        )
        .await
        .context("Failed to get StateChainGateway address from SC")?
        .into();

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get KeyManager address from SC")?
		.into();

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get Vault contract address from SC")?
		.into();

	let eth_client = EthersRetryRpcClient::new(
		scope,
		EthRpcClient::new(settings).await?,
		ReconnectSubscriptionClient::new(settings.ws_node_endpoint.clone(), expected_chain_id),
	);

	let eth_source = EthSource::new(eth_client.clone()).shared(scope);

	let eth_chain_tracking = eth_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), eth_client.clone())
		.run();

	scope.spawn(async move {
		eth_chain_tracking.await;
		Ok(())
	});

	let eth_safe_vault_source = eth_source
		.strictly_monotonic()
		.lag_safety(SAFETY_MARGIN)
		.shared(scope)
		.chunk_by_vault(epoch_source.vaults().await);

	let key_manager_witnesser = eth_safe_vault_source
		.clone()
		.key_manager_witnessing(state_chain_client.clone(), eth_client.clone(), key_manager_address)
		.continuous("KeyManager".to_string(), db.clone())
		.run();

	scope.spawn(async move {
		key_manager_witnesser.await;
		Ok(())
	});

	let state_chain_gateway_witnesser = eth_safe_vault_source
		.clone()
		.state_chain_gateway_witnessing(
			state_chain_client.clone(),
			eth_client.clone(),
			state_chain_gateway_address,
		)
		.continuous("StateChainGateway".to_string(), db.clone())
		.run();

	scope.spawn(async move {
		state_chain_gateway_witnesser.await;
		Ok(())
	});

	let usdc_deposit_witnesser = eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, UsdcEvents>(
			state_chain_client.clone(),
			eth_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Usdc,
		)
		.await
		.continuous("USDCDeposits".to_string(), db.clone())
		.run();

	scope.spawn(async move {
		usdc_deposit_witnesser.await;
		Ok(())
	});

	let flip_deposit_witnesser = eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, FlipEvents>(
			state_chain_client.clone(),
			eth_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Flip,
		)
		.await
		.continuous("FlipDeposits".to_string(), db.clone())
		.run();

	scope.spawn(async move {
		flip_deposit_witnesser.await;
		Ok(())
	});

	let ethereum_deposits_witnesser = eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.ethereum_deposits(state_chain_client.clone(), eth_client.clone())
		.await
		.continuous("EthereumDeposits".to_string(), db.clone())
		.run();

	scope.spawn(async move {
		ethereum_deposits_witnesser.await;
		Ok(())
	});

	let vault_witnesser = eth_safe_vault_source
		.vault_witnessing(state_chain_client.clone(), eth_client.clone(), vault_address)
		.continuous("Vault".to_string(), db)
		.run();

	scope.spawn(async move {
		vault_witnesser.await;
		Ok(())
	});

	Ok(())
}
