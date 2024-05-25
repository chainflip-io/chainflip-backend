mod chain_tracking;
mod sol_deposits;
mod source;

use std::sync::Arc;

use cf_primitives::EpochIndex;
use futures_core::Future;
use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	sol::retry_rpc::SolRetryRpcClient,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};

use super::{
	common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
	sol::source::SolSource,
};

use anyhow::{Context, Result};

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	sol_client: SolRetryRpcClient,
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
	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::SolanaVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let sol_source = SolSource::new(sol_client.clone()).strictly_monotonic().shared(scope);

	sol_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), sol_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults::<cf_chains::Solana>().await;

	// ===== Full witnessing stream =====

	let sol_safe_vault_source = sol_source
		// .lag_safety(sol_safety_margin) // NO SAFETY MARGIN
		.logging("safe block produced")
		.chunk_by_vault(vaults, scope);

	let sol_safe_vault_source_deposit_addresses = sol_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	// sol_safe_vault_source_deposit_addresses
	// 	.clone()
	// 	.erc20_deposits::<_, _, _, UsdtEvents>(
	// 		process_call.clone(),
	// 		sol_client.clone(),
	// 		cf_primitives::chains::assets::sol::Asset::Usdc,
	// 		usdt_contract_address,
	// 	)
	// 	.await?
	// 	.continuous("USDTDeposits".to_string(), db.clone())
	// 	.logging("USDTDeposits")
	// 	.spawn(scope);

	sol_safe_vault_source_deposit_addresses
		.clone()
		.solana_deposits(
			process_call.clone(),
			sol_client.clone(),
			cf_primitives::chains::assets::sol::Asset::Sol,
		)
		.await
		.continuous("SolanaDeposits".to_string(), db.clone())
		.logging("SolanaDeposits")
		.spawn(scope);

	Ok(())
}
