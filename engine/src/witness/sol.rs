mod chain_tracking;
mod nonce_witnessing;
mod sol_deposits;
mod source;

use std::{
	collections::{HashMap, HashSet},
	str::FromStr,
	sync::Arc,
};

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
use cf_chains::sol::{SolAddress, SolHash, SolSignature, LAMPORTS_PER_SIGNATURE};

use crate::common::Mutex;
use anyhow::{Context, Result};
use state_chain_runtime::SolanaInstance;

pub async fn process_egress<ProcessCall, ProcessingFut>(
	epoch: Vault<cf_chains::Solana, (), ()>,
	header: Header<u64, SolHash, ((), Vec<(SolSignature, u64)>)>,
	process_call: ProcessCall,
	sol_client: SolRetryRpcClient,
	cached_witnessed_egresses: Arc<Mutex<HashSet<SolSignature>>>,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let (_, monitored_egresses) = header.data;

	let monitored_tx_signatures = monitored_egresses.into_iter().map(|(x, _)| x).collect();

	let success_witnesses_result = success_witnesses(&sol_client, monitored_tx_signatures).await;

	let mut cached_witnessed_egresses = cached_witnessed_egresses.lock().await;

	for (tx_signature, _slot, tx_fee) in success_witnesses_result {
		// Cache submitted signatures to not keep submitting the signatures until there is
		// consensus on the State Chain, as the engine will keep polling the same transactions.
		if !cached_witnessed_egresses.contains(&tx_signature) {
			// TODO: Not submitting the slot?
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
			cached_witnessed_egresses.insert(tx_signature);
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
		.get_signature_statuses(monitored_tx_signatures.as_slice(), true)
		.await
		.value;

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

	let mut finalized_txs_info = Vec::new();

	println!("Finalized transactions: {:?}", finalized_transactions);

	// We could run this queries concurrently to make it faster but we'll have few txs anyway
	for (signature, slot) in finalized_transactions {
		let transaction = sol_client
			.get_transaction(
				&signature,
				RpcTransactionConfig {
					encoding: Some(UiTransactionEncoding::Json),
					// Using finalized there could be a race condition where this doesn't get
					// us the tx. But "Processed" is timing out so we better retry with finalized.
					commitment: Some(CommitmentConfig::finalized()),
					// Getting also type 0 even if we don't use them atm
					max_supported_transaction_version: Some(0),
				},
			)
			.await;
		println!("Transaction: {:?}", transaction);

		let fee = match transaction.transaction.meta {
			Some(meta) => meta.fee,
			// This shouldn't happen. Want to avoid Erroring. We either default to 5000 or return
			// OK(()) so we don't submit transaction_succeeded and retry again later. Defaulting to
			// avoid potentially getting stuck not witness something because no meta is returned.
			// TODO: Check if this approach makes sense.
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
	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::SolanaVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let supported_sol_token_assets: HashMap<cf_primitives::chains::assets::sol::Asset, SolAddress> =
		state_chain_client
			.storage_map::<pallet_cf_environment::SolanaSupportedAssets<state_chain_runtime::Runtime>, _>(
				state_chain_client.latest_finalized_block().hash,
			)
			.await
			.context("Failed to fetch Solana supported assets")?;

	let usdc_address = *supported_sol_token_assets
		.get(&cf_primitives::chains::assets::sol::Asset::SolUsdc)
		.context("SolanaSupportedAssets does not include USDC")?;

	// TODO: Get this from the environment once implemented
	let nonces_accounts: Vec<(SolAddress, SolHash)> = vec![(
		SolAddress::from_str("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo").unwrap(),
		SolHash::from_str("4UjUkFWp1Zkdge8zkuptMqHG9Xwr5eWbyperujHAHiNC").unwrap(),
	)];

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

	// TODO: Probably use DB instead.
	// If in the new pallet the cumulative historical amount is tracked we could maybe get away with
	// using cache but upon restart the engine would submit an extrinsic for each deposit address.
	let cached_balances = Arc::new(Mutex::new(HashMap::new()));
	// Similar when it comes to witness egresses. However, we could get away with a simple cache
	// since egressed transactions have a shorter lifespan than deposits and there won't be many of
	// simultaneously.
	let cached_witnessed_egresses = Arc::new(Mutex::new(HashSet::new()));

	sol_safe_vault_source_deposit_addresses
		.clone()
		.sol_deposits(
			process_call.clone(),
			sol_client.clone(),
			cf_primitives::chains::assets::sol::Asset::Sol,
			vault_address,
			cached_balances.clone(),
			None,
		)
		.await
		.continuous("SolanaDeposits".to_string(), db.clone())
		.logging("SolanaDeposits")
		.spawn(scope);

	sol_safe_vault_source_deposit_addresses
		.clone()
		.sol_deposits(
			process_call.clone(),
			sol_client.clone(),
			cf_primitives::chains::assets::sol::Asset::SolUsdc,
			vault_address,
			cached_balances,
			Some(usdc_address),
		)
		.await
		.continuous("SolanaUsdcDeposits".to_string(), db.clone())
		.logging("SolanaUsdcDeposits")
		.spawn(scope);

	// Witnessing the state of the nonce accounts periodically. It could also be done
	// only when a broadcast is witnessedd, which is the only time a nonce account
	// might change. Doing it periocally is more reliable to ensure we don't miss a
	// change in the value but will require an extra rpc call.
	// TODO: Should we witness nonces through chunk_by_time and not chunk_by_vault?
	sol_safe_vault_source_deposit_addresses
		.clone()
		.witness_nonces(process_call.clone(), sol_client.clone(), nonces_accounts)
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
			let cached_witnessed_egresses = cached_witnessed_egresses.clone();
			move |epoch, header| {
				process_egress(
					epoch,
					header,
					process_call.clone(),
					sol_client.clone(),
					cached_witnessed_egresses.clone(),
				)
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
