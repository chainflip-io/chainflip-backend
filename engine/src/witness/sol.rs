mod chain_tracking;
mod nonce_witnessing;
mod sol_deposits;
mod source;

use std::{collections::HashMap, sync::Arc};

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
		rpc_client_api::{
			RpcTransactionConfig, TransactionConfirmationStatus, UiTransactionEncoding,
		},
	},
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};
use cf_chains::sol::{SolHash, SolSignature, LAMPORTS_PER_SIGNATURE};

use crate::common::Mutex;
use anyhow::{Context, Result};
use state_chain_runtime::SolanaInstance;

pub async fn process_egress<ProcessCall, ProcessingFut>(
	epoch: Vault<cf_chains::Solana, (), ()>,
	header: Header<u64, SolHash, ((), Vec<(SolSignature, u64)>)>,
	process_call: ProcessCall,
	sol_client: SolRetryRpcClient,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let (_, monitored_egresses) = header.data;

	let monitored_tx_signatures: Vec<_> = monitored_egresses.into_iter().map(|(x, _)| x).collect();

	if !monitored_tx_signatures.is_empty() {
		let success_witnesses_result =
			success_witnesses(&sol_client, monitored_tx_signatures).await;

		for (tx_signature, _slot, tx_fee) in success_witnesses_result {
			process_call(
				pallet_cf_broadcast::Call::<_, SolanaInstance>::transaction_succeeded {
					tx_out_id: tx_signature,
					signer_id: epoch.info.0,
					tx_fee,
					tx_metadata: (),
					transaction_ref: tx_signature,
				}
				.into(),
				epoch.index,
			)
			.await;
		}
	}
}

async fn success_witnesses(
	sol_client: &SolRetryRpcClient,
	monitored_tx_signatures: Vec<SolSignature>,
) -> Vec<(SolSignature, u64, u64)>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let mut finalized_transactions = Vec::new();

	let signature_statuses = sol_client
		.get_signature_statuses(monitored_tx_signatures.as_slice(), false)
		.await
		.value;

	for (signature, status_option) in
		monitored_tx_signatures.iter().zip(signature_statuses.into_iter())
	{
		if let Some(status) = status_option {
			// For now we don't check if the transaction have reverted, as we don't handle it in the
			// SC.
			if let Some(TransactionConfirmationStatus::Finalized) = status.confirmation_status {
				finalized_transactions.push((*signature, status.slot));
			}
		}
	}

	let mut finalized_txs_info = Vec::new();

	// We could run this queries concurrently but we'll have few txs anyway
	for (signature, slot) in finalized_transactions {
		let transaction = sol_client
			.get_transaction(
				&signature,
				RpcTransactionConfig {
					encoding: Some(UiTransactionEncoding::Json),
					// Using finalized there could be a race condition where this doesn't get
					// the tx. But "Processed" is timing out so we better retry with finalized.
					commitment: Some(CommitmentConfig::finalized()),
					// Getting also type 0 even if we don't use them atm
					max_supported_transaction_version: Some(0),
				},
			)
			.await;

		let fee = match transaction.transaction.meta {
			Some(meta) => meta.fee,
			// This shouldn't happen. Want to avoid Erroring. We either default to 5000 or return
			// OK(()) so we don't submit transaction_succeeded and retry again later. Defaulting to
			// avoid potentially getting stuck not witness something because no meta is returned.
			None => LAMPORTS_PER_SIGNATURE,
		};

		finalized_txs_info.push((signature, slot, fee));
	}

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
	let sol_env = state_chain_client
		.storage_value::<pallet_cf_environment::SolanaApiEnvironment<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Solana Environment from SC")?;

	let sol_source = SolSource::new(sol_client.clone()).strictly_monotonic().shared(scope);

	sol_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), sol_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let vaults = epoch_source.vaults::<cf_chains::Solana>().await;

	// ===== Full witnessing stream =====

	// Not using safety margin in Solana
	let sol_safe_vault_source =
		sol_source.logging("safe block produced").chunk_by_vault(vaults, scope);

	let sol_safe_vault_source_deposit_addresses = sol_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	// In the new pallet we'll be submitting to cumulative historical balance of each deposit
	// channel. We cache the balance of deposit channels to avoid submitting them on every query.
	// Upon restarting the CFE all the balances will be resubmitted, as the cache is not persistent
	// and the pallet will handle it correctly.
	let cached_balances = Arc::new(Mutex::new(HashMap::new()));

	sol_safe_vault_source_deposit_addresses
		.clone()
		.sol_deposits(
			process_call.clone(),
			sol_client.clone(),
			sol_env.vault_program,
			cached_balances.clone(),
			sol_env.usdc_token_mint_pubkey,
		)
		.await
		.continuous("SolanaDeposits".to_string(), db.clone())
		.logging("SolanaDeposits")
		.spawn(scope);

	// Witnessing the state of the nonce accounts periodically. It could also be done only when a
	// broadcast is witnessed, which is the only time a nonce account should change. Doing it
	// periocally is more reliable to ensure we don't miss a change in the value but will require an
	// extra rpc call.
	sol_safe_vault_source_deposit_addresses
		.clone()
		.witness_nonces(process_call.clone(), sol_client.clone(), state_chain_client.clone())
		.await
		.continuous("SolanaNonceWitnessing".to_string(), db.clone())
		.logging("SolanaNonceWitnessing")
		.spawn(scope);

	sol_safe_vault_source
		.clone()
		.egress_items(scope, state_chain_stream, state_chain_client.clone())
		.await
		.then({
			let process_call = process_call.clone();
			let sol_client = sol_client.clone();
			move |epoch, header| {
				process_egress(epoch, header, process_call.clone(), sol_client.clone())
			}
		})
		.continuous("SolanaEgress".to_string(), db)
		.logging("Egress")
		.spawn(scope);

	Ok(())
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{NodeContainer, WsHttpEndpoints},
		sol::retry_rpc::SolRetryRpcClient,
	};

	use cf_chains::{Chain, Solana};
	use futures_util::FutureExt;
	use std::str::FromStr;
	use utilities::task_scope;

	use super::*;

	#[tokio::test]
	async fn test_success_witnesses() {
		task_scope::task_scope(|scope| {
			async {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "wss://api.devnet.solana.com".into(),
							http_endpoint: "https://api.devnet.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let monitored_tx_signatures = vec![
					SolSignature::from_str(
						"4udChXyRXrqBxUTr9F3nbTcPyvteLJtFQ3wM35J53NdP4GWwUp2wBwdTJEYs2aiNz7DyCqitok6ci7qqHPkRByb2").unwrap()
				];

				let result =
					success_witnesses(&retry_client, monitored_tx_signatures.clone()).await;
				println!("{:?}", result);
				assert_eq!(result.len(), 1);
				assert_eq!(result[0].0, monitored_tx_signatures[0]);
				assert!(result[0].1 > 0);
				assert_eq!(result[0].2, 5000);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
