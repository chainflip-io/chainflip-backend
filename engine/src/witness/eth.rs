mod contract_common;
mod erc20_deposits;
mod eth_chain_tracking;
mod eth_source;
mod ethereum_deposits;
mod key_manager;
mod state_chain_gateway;
pub mod vault;

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
	witness::eth::erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents},
};

use super::common::{
	chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder,
	STATE_CHAIN_CONNECTION,
};
use eth_source::EthSource;
use vault::EthAssetApi;

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
        .context("Failed to get StateChainGateway address from SC")?;

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let eth_client = EthersRetryRpcClient::new(
		scope,
		EthRpcClient::new(settings).await?,
		ReconnectSubscriptionClient::new(settings.ws_node_endpoint.clone(), expected_chain_id),
	);

	let eth_source = EthSource::new(eth_client.clone()).shared(scope);

	eth_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), eth_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let eth_safe_vault_source = eth_source
		.strictly_monotonic()
		.lag_safety(SAFETY_MARGIN)
		.logging("safe block produced")
		.shared(scope)
		.chunk_by_vault(epoch_source.vaults().await);

	eth_safe_vault_source
		.clone()
		.key_manager_witnessing(state_chain_client.clone(), eth_client.clone(), key_manager_address)
		.continuous("KeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	eth_safe_vault_source
		.clone()
		.state_chain_gateway_witnessing(
			state_chain_client.clone(),
			eth_client.clone(),
			state_chain_gateway_address,
		)
		.continuous("StateChainGateway".to_string(), db.clone())
		.logging("StateChainGateway")
		.spawn(scope);

	eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, UsdcEvents>(
			state_chain_client.clone(),
			eth_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Usdc,
		)
		.await?
		.continuous("USDCDeposits".to_string(), db.clone())
		.logging("USDCDeposits")
		.spawn(scope);

	eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, FlipEvents>(
			state_chain_client.clone(),
			eth_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Flip,
		)
		.await?
		.continuous("FlipDeposits".to_string(), db.clone())
		.logging("FlipDeposits")
		.spawn(scope);

	eth_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.ethereum_deposits(state_chain_client.clone(), eth_client.clone())
		.await
		.continuous("EthereumDeposits".to_string(), db.clone())
		.logging("EthereumDeposits")
		.spawn(scope);

	eth_safe_vault_source
		.vault_witnessing(state_chain_client.clone(), eth_client.clone(), vault_address)
		.continuous("Vault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}
