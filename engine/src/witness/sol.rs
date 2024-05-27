mod chain_tracking;
mod sol_deposits;
mod source;

use std::sync::Arc;

use cf_primitives::EpochIndex;
use futures_core::Future;
use utilities::task_scope::Scope;

use super::{
	common::{
		chain_source::{extension::ChainSourceExt, Header},
		epoch_source::{EpochSourceBuilder, Vault},
	},
	sol::source::SolSource,
};
use crate::{
	db::PersistentKeyDB,
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::{RpcTransactionConfig, TransactionConfirmationStatus},
	},
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};
use cf_chains::sol::{SolAddress, SolHash, SolSignature};

use anyhow::{anyhow, Context, Result};
use futures::{future::join_all, stream};

// pub async fn process_egress<ProcessCall, ProcessingFut>(
// 	epoch: Vault<cf_chains::Solana, SolAddress, ()>,
// 	header: Header<u64, SolHash, Vec<SolSignature>>,
// 	process_call: ProcessCall,
// ) where
// 	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
// 		+ Send
// 		+ Sync
// 		+ Clone
// 		+ 'static,
// 	ProcessingFut: Future<Output = ()> + Send + 'static,
// {
// 	let test = header.data;
// }

async fn success_witnesses(
	sol_client: &SolRetryRpcClient,
	monitored_tx_signatures: &[SolSignature],
	_nonce_accounts: &[SolAddress], // can be pulled from the environment
) -> Result<Vec<(SolSignature, u64, u64)>, anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let mut finalized_transactions = Vec::new();

	let signature_statuses =
		sol_client.get_signature_statuses(monitored_tx_signatures, true).await.value;

	for (signature, status_option) in
		monitored_tx_signatures.iter().zip(signature_statuses.into_iter())
	{
		if let Some(status) = status_option {
			// For now we don't check if the transaction have errored out, as CCM could fail.
			if let Some(TransactionConfirmationStatus::Finalized) = status.confirmation_status {
				finalized_transactions.push((*signature, status.slot));
			}
		}
	}

	// We can either try to get the correct nonce account from the succesful transactions, it's
	// probably always in the same key position, since we need to call get_transaction anyway to get
	// the fees. Otherwise we just use get_multiple_accounts for all the nonce accounts and compare
	// them with the the previous hashes, which we can also pull from the environment.
	// Matching keys from successful transactions against nonces is dangerous because ccm calls
	// might have some sneaky keys passed by the user.
	let futures: Vec<_> = finalized_transactions
		.into_iter()
		.map(|(signature, slot)| {
			async move {
				let transaction = sol_client
					.get_transaction(
						&signature,
						RpcTransactionConfig {
							encoding: None, // default Json
							// Using confirmed to prevent race conditions. We assume that if we saw
							// it finalized in get_signature_statuses it will eventually be
							// finalized
							commitment: Some(CommitmentConfig::confirmed()),
							// Getting also type 0 even if we don't use them atm
							max_supported_transaction_version: Some(0),
						},
					)
					.await;
				let fee = match transaction.transaction.meta {
					Some(meta) => meta.fee,
					None => return Err(anyhow!("Empty meta")),
				};
				Ok((signature, slot, fee))
			}
		})
		.collect();

	let finalized_txs_info: Result<Vec<_>, _> = join_all(futures).await.into_iter().collect();

	finalized_txs_info
}

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
			vault_address,
		)
		.await
		.continuous("SolanaDeposits".to_string(), db.clone())
		.logging("SolanaDeposits")
		.spawn(scope);

	// sol_safe_vault_source_deposit_addresses
	// 	.clone()
	// 	.egress_items(scope, state_chain_stream, state_chain_client.clone())
	// 	.await
	// 	.then({
	// 		let process_call = process_call.clone();
	// 		move |epoch, header| process_egress(epoch, header, process_call.clone())
	// 	})
	// 	.continuous("SolanaEgress".to_string(), db)
	// 	.logging("Egress")
	// 	.spawn(scope);

	Ok(())
}
