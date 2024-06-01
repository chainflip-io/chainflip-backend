mod chain_tracking;
mod nonce_witnessing;
mod sol_deposits;
mod source;

use std::{collections::HashMap, str::FromStr, sync::Arc};

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

// TODO: We will keep resubmitting the same tx signatures again and again until we reach consensus
// and the tx is no longer being pulled from the SC. We could keep a cache of the tx signatures we
// have already submitted or the block number of the latest signature seen. The last one be
// problematic if someone frontruns us submitting the tx.
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

	let monitored_tx_signatures = monitored_egresses.into_iter().map(|(x, _)| x).collect();

	let success_witnesses_result = success_witnesses(&sol_client, monitored_tx_signatures).await;

	for (tx_signature, _slot, tx_fee) in success_witnesses_result {
		// TODO: Should we submit the slot instead of the epoch.index?
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

	// Regarding the option of nonces witnesssing on broadcast:
	// We could either try to get the correct nonce account from the succesful transactions, it's
	// probably always in the same key position, since we need to call get_transaction anyway to get
	// the fees. Otherwise we just use get_multiple_accounts for all the nonce accounts and compare
	// them with the the previous hashes, which we can also pull from the environment.
	// Matching keys from successful transactions against nonces is dangerous because ccm calls
	// might have some sneaky keys passed by the user.

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

	// TODO: Get this from the environment
	let usdc_pubkey = SolAddress::from_str("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p")
		.expect("Failed to get USDC contract address from SC");
	let nonces_accounts: Vec<(SolAddress, SolHash)> = vec![(
		SolAddress::from_str("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo").expect("Temp"),
		SolHash::from_str("4UjUkFWp1Zkdge8zkuptMqHG9Xwr5eWbyperujHAHiNC").expect("Temp"),
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

	let sol_safe_vault_source = sol_source
		// .lag_safety(sol_safety_margin) // NO SAFETY MARGIN
		// .logging("safe block produced")
		.chunk_by_vault(vaults, scope);

	let sol_safe_vault_source_deposit_addresses = sol_safe_vault_source
		.clone()
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await;

	// TODO: Probably use DB instead.
	// Using this as a global variable to store the previous balances. The new pallet it
	// might be alright if we submit values again after a restart, it's just not efficient.
	// Currently with this workaround to make it work for the current pallet, a restart of the
	// engine would cause the engine to trigger swaps on all deposit channels with balances.
	let cached_balances = Arc::new(Mutex::new(HashMap::new()));

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
			Some(usdc_pubkey),
		)
		.await
		.continuous("SolanaUsdcDeposits".to_string(), db.clone())
		.logging("SolanaUsdcDeposits")
		.spawn(scope);

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
		// use settings:: Settings
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
				// let settings = Settings::new_test().unwrap();
				// let client = SolRetryRpcClient::<SolRpcClient>::new(
				// 	scope,
				// 	settings.sol.nodes,
				// 	U256::from(1337u64),
				// 	"sol_rpc",
				// 	"sol_subscribe",
				// 	"Ethereum",
				// 	Ethereum::WITNESS_PERIOD,
				// )
				// .unwrap();

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
