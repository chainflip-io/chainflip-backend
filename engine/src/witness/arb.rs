mod arb_chain_tracking;
pub mod source;

use std::{collections::HashMap, sync::Arc};

use sp_core::H160;
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
	witness::evm::erc20_deposits::usdc::UsdcEvents,
};

use self::source::ArbSource;

use super::common::{
	chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder,
	STATE_CHAIN_CONNECTION,
};

use anyhow::{Context, Result};

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Eth,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
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
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_hash(),
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_arb_erc20_assets: HashMap<cf_primitives::chains::assets::arb::Asset, H160> =
		state_chain_client
			.storage_map::<pallet_cf_environment::ArbitrumSupportedAssets<state_chain_runtime::Runtime>, _>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.context("Failed to fetch Arbitrum supported assets")?;

	let usdc_contract_address = *supported_arb_erc20_assets
		.get(&cf_primitives::chains::assets::arb::Asset::ArbUsdc)
		.context("ArbitrumSupportedAssets does not include USDC")?;

	let supported_arb_erc20_assets: HashMap<H160, cf_primitives::Asset> =
		supported_arb_erc20_assets
			.into_iter()
			.map(|(asset, address)| (address, asset.into()))
			.collect();

	let arb_client = EthersRetryRpcClient::new(
		scope,
		EthRpcClient::new(settings).await?,
		ReconnectSubscriptionClient::new(settings.ws_node_endpoint.clone(), expected_chain_id),
	);

	let arb_source = ArbSource::new(arb_client.clone()).shared(scope);

	arb_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), arb_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let arb_safe_vault_source = arb_source
		.strictly_monotonic()
		.logging("safe block produced")
		.shared(scope)
		.chunk_by_vault(epoch_source.vaults().await);

	arb_safe_vault_source
		.clone()
		.key_manager_witnessing(state_chain_client.clone(), arb_client.clone(), key_manager_address)
		.continuous("ArbKeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.erc20_deposits::<_, _, UsdcEvents>(
			state_chain_client.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbUsdc,
			usdc_contract_address,
		)
		.await?
		.continuous("ArbUSDCDeposits".to_string(), db.clone())
		.logging("USDCDeposits")
		.spawn(scope);

	arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.ethereum_deposits(
			state_chain_client.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbEth,
			address_checker_address,
			vault_address,
		)
		.await
		.continuous("ArbEthereumDeposits".to_string(), db.clone())
		.logging("EthereumDeposits")
		.spawn(scope);

	arb_safe_vault_source
		.vault_witnessing(
			state_chain_client.clone(),
			arb_client.clone(),
			vault_address,
			cf_primitives::Asset::ArbEth,
			cf_primitives::ForeignChain::Arbitrum,
			supported_arb_erc20_assets,
		)
		.continuous("ArbVault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}
