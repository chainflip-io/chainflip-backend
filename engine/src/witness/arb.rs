mod chain_tracking;

use std::{collections::HashMap, sync::Arc};

use cf_chains::Arbitrum;
use cf_primitives::EpochIndex;
use futures_core::Future;
use sp_core::H160;
use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	eth::{retry_rpc::EthRetryRpcClient, rpc::EthRpcSigningClient},
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
		STATE_CHAIN_CONNECTION,
	},
	witness::evm::erc20_deposits::usdc::UsdcEvents,
};

use super::{
	common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
	evm::source::EvmSource,
};

use anyhow::{Context, Result};

use chainflip_node::chain_spec::berghain::ARBITRUM_SAFETY_MARGIN;

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	arb_client: EthRetryRpcClient<EthRpcSigningClient>,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_arb_erc20_assets: HashMap<cf_primitives::chains::assets::arb::Asset, H160> =
		state_chain_client
			.storage_map::<pallet_cf_environment::ArbitrumSupportedAssets<state_chain_runtime::Runtime>, _>(
				state_chain_client.latest_finalized_block().hash,
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

	let arb_source = EvmSource::<_, Arbitrum>::new(arb_client.clone())
		.strictly_monotonic()
		.shared(scope);

	arb_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), arb_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults::<Arbitrum>().await;

	// ===== Full witnessing stream =====

	let arb_safety_margin = state_chain_client
		.storage_value::<pallet_cf_ingress_egress::WitnessSafetyMargin<
			state_chain_runtime::Runtime,
			state_chain_runtime::ArbitrumInstance,
		>>(state_chain_stream.cache().hash)
		.await?
		// Default to berghain in case the value is missing (e.g. during initial upgrade)
		.unwrap_or(ARBITRUM_SAFETY_MARGIN);

	tracing::info!("Safety margin for Arbitrum is set to {arb_safety_margin} blocks.",);

	let arb_safe_vault_source = arb_source
		.lag_safety(arb_safety_margin as usize)
		.logging("safe block produced")
		.chunk_by_vault(vaults, scope);

	let arb_safe_vault_source_deposit_addresses = arb_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	arb_safe_vault_source
		.clone()
		.key_manager_witnessing(process_call.clone(), arb_client.clone(), key_manager_address)
		.continuous("KeyManager".to_string(), db.clone())
		.logging("KeyManager")
		.spawn(scope);

	arb_safe_vault_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			process_call.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbUsdc,
			usdc_contract_address,
		)
		.await?
		.continuous("USDCDeposits".to_string(), db.clone())
		.logging("USDCDeposits")
		.spawn(scope);

	arb_safe_vault_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			process_call.clone(),
			arb_client.clone(),
			cf_primitives::chains::assets::arb::Asset::ArbEth,
			address_checker_address,
			vault_address,
		)
		.await
		.continuous("EthereumDeposits".to_string(), db.clone())
		.logging("EthereumDeposits")
		.spawn(scope);

	arb_safe_vault_source
		.vault_witnessing(
			process_call,
			arb_client.clone(),
			vault_address,
			cf_primitives::Asset::ArbEth,
			cf_primitives::ForeignChain::Arbitrum,
			supported_arb_erc20_assets,
		)
		.continuous("Vault".to_string(), db)
		.logging("Vault")
		.spawn(scope);

	Ok(())
}
