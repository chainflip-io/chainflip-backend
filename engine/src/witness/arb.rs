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
	witness::eth::{erc20_deposits::usdc::UsdcEvents, eth_source::EthSource, vault::EthAssetApi},
};

use super::common::{
	chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder,
	STATE_CHAIN_CONNECTION,
};

use anyhow::{Context, Result};

const SAFETY_MARGIN: usize = 7;

// Most of this is the same as engine/src/witness/eth.rs
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
			.storage_value::<pallet_cf_environment::ArbitrumChainId<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.expect(STATE_CHAIN_CONNECTION),
	);

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get KeyManager address from SC")?
		.into();

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get Vault contract address from SC")?
		.into();

	let arb_client = EthersRetryRpcClient::new(
		scope,
		EthRpcClient::new(settings).await?,
		ReconnectSubscriptionClient::new(settings.ws_node_endpoint.clone(), expected_chain_id),
	);

	let arb_source = EthSource::new(arb_client.clone()).shared(scope);

	arb_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), arb_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let arb_safe_vault_source = arb_source
		.strictly_monotonic()
		.lag_safety(SAFETY_MARGIN)
		.logging("safe block produced")
		.shared(scope)
		.chunk_by_vault(epoch_source.vaults().await);

	arb_safe_vault_source
		.clone()
		.key_manager_witnessing(state_chain_client.clone(), arb_client.clone(), key_manager_address)
		.continuous("KeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, UsdcEvents>(
			state_chain_client.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::eth::Asset::Usdc,
		)
		.await?
		.continuous("ArbUSDCDeposits".to_string(), db.clone())
		.logging("ArbUSDCDeposits")
		.spawn(scope);

	arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.ethereum_deposits(state_chain_client.clone(), arb_client.clone())
		.await
		.continuous("ArbEthereumDeposits".to_string(), db.clone())
		.logging("ArbEthereumDeposits")
		.spawn(scope);

	arb_safe_vault_source
		.vault_witnessing(state_chain_client.clone(), arb_client.clone(), vault_address)
		.continuous("Vault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}
