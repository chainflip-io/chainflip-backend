mod nonce_witnessing;
mod sol_deposits;

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::{
			EncodedTransaction, RpcTransactionConfig, TransactionConfirmationStatus, UiMessage,
			UiTransactionEncoding,
		},
	},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::sol::sol_deposits::get_channel_ingress_amounts,
};
use anyhow::{anyhow, Result};
use cf_chains::sol::{SolAddress, SolSignature, LAMPORTS_PER_SIGNATURE};
use futures::FutureExt;
use pallet_cf_elections::{electoral_system::ElectoralSystem, vote_storage::VoteStorage};
use state_chain_runtime::{
	chainflip::solana_elections::{
		SolanaBlockHeightTracking, SolanaEgressWitnessing, SolanaElectoralSystem,
		SolanaFeeTracking, SolanaIngressTracking, SolanaNonceTracking, TransactionSuccessDetails,
	},
	SolanaInstance,
};

use std::{str::FromStr, sync::Arc};
use utilities::{context, task_scope, task_scope::Scope};

#[derive(Clone)]
struct SolanaBlockHeightTrackingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaBlockHeightTracking> for SolanaBlockHeightTrackingVoter {
	async fn vote(
		&self,
		_settings: <SolanaBlockHeightTracking as ElectoralSystem>::ElectoralSettings,
		_properties: <SolanaBlockHeightTracking as ElectoralSystem>::ElectionProperties,
	) -> Result<
		<<SolanaBlockHeightTracking as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		Ok(self.client.get_slot(CommitmentConfig::finalized()).await)
	}
}

#[derive(Clone)]
struct SolanaFeeTrackingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaFeeTracking> for SolanaFeeTrackingVoter {
	async fn vote(
		&self,
		_settings: <SolanaFeeTracking as ElectoralSystem>::ElectoralSettings,
		_properties: <SolanaFeeTracking as ElectoralSystem>::ElectionProperties,
	) -> Result<<<SolanaFeeTracking as ElectoralSystem>::Vote as VoteStorage>::Vote, anyhow::Error>
	{
		let priorization_fees = self.client.get_recent_prioritization_fees().await;

		let mut priority_fees: Vec<u64> =
			priorization_fees.iter().map(|f| f.prioritization_fee).collect();
		priority_fees.sort();

		Ok(context!(priority_fees.get(priority_fees.len().saturating_sub(1) / 2).cloned())?)
	}
}

#[derive(Clone)]
struct SolanaIngressTrackingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaIngressTracking> for SolanaIngressTrackingVoter {
	async fn vote(
		&self,
		settings: <SolanaIngressTracking as ElectoralSystem>::ElectoralSettings,
		properties: <SolanaIngressTracking as ElectoralSystem>::ElectionProperties,
	) -> Result<
		<<SolanaIngressTracking as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		get_channel_ingress_amounts(
			&self.client,
			settings.vault_program,
			settings.usdc_token_mint_pubkey,
			properties,
		)
		.await
		.and_then(|vote| {
			vote.try_into().map_err(|_| anyhow::anyhow!("Too many channels in election"))
		})
	}
}

#[derive(Clone)]
struct SolanaNonceTrackingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaNonceTracking> for SolanaNonceTrackingVoter {
	async fn vote(
		&self,
		_settings: <SolanaNonceTracking as ElectoralSystem>::ElectoralSettings,
		properties: <SolanaNonceTracking as ElectoralSystem>::ElectionProperties,
	) -> Result<<<SolanaNonceTracking as ElectoralSystem>::Vote as VoteStorage>::Vote, anyhow::Error>
	{
		let (nonce_account, _previous_nonce) = properties;
		let (response_account, response_nonce) =
			nonce_witnessing::get_durable_nonces(&self.client, vec![nonce_account])
				.await?
				.pop()
				.expect("If the query succeeds, we expect a nonce for the account we queried for");

		assert_eq!(response_account, nonce_account);

		Ok(response_nonce)
	}
}

#[derive(Clone)]
struct SolanaEgressWitnessingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaEgressWitnessing> for SolanaEgressWitnessingVoter {
	async fn vote(
		&self,
		_settings: <SolanaEgressWitnessing as ElectoralSystem>::ElectoralSettings,
		signature: <SolanaEgressWitnessing as ElectoralSystem>::ElectionProperties,
	) -> Result<
		<<SolanaEgressWitnessing as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		let (sig, _slot, tx_fee, signer) = success_witnesses(&self.client, vec![signature])
			.await
			.into_iter()
			.next()
			.ok_or(anyhow!("Success querying for {signature} but no items"))?;
		assert_eq!(sig, signature, "signature we requested should be the same as in the response");
		// TODO: Update TransactionSuccessDetails to take an option and not unwrap here
		Ok(TransactionSuccessDetails { tx_fee, signer: signer.unwrap_or_default() })
	}
}

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: SolRetryRpcClient,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<SolanaInstance>
		+ 'static
		+ Send
		+ Sync,
{
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<SolanaElectoralSystem, _>::new((
						SolanaBlockHeightTrackingVoter { client: client.clone() },
						SolanaFeeTrackingVoter { client: client.clone() },
						SolanaIngressTrackingVoter { client: client.clone() },
						SolanaNonceTrackingVoter { client: client.clone() },
						SolanaEgressWitnessingVoter { client },
					)),
				)
				.continuously_vote()
				.await;

				Ok(())
			}
			.boxed()
		})
		.await
	});

	Ok(())
}

async fn success_witnesses(
	sol_client: &SolRetryRpcClient,
	monitored_tx_signatures: Vec<SolSignature>,
) -> Vec<(SolSignature, u64, u64, Option<SolAddress>)>
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
			// For now we don't check if the transaction reverted, as we don't handle it in the SC.
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

		let fee = match &transaction.transaction.meta {
			Some(meta) => meta.fee,
			// This shouldn't happen. Want to avoid Erroring. We either default to 5000 or return
			// OK(()) so we don't submit transaction_succeeded and retry again later. Defaulting to
			// avoid potentially getting stuck not witness something because no meta is returned.
			None => LAMPORTS_PER_SIGNATURE,
		};

		let signer =
			if let EncodedTransaction::Json(ui_transaction) = transaction.transaction.transaction {
				match ui_transaction.message {
					UiMessage::Parsed(message) => message
						.account_keys
						.get(0)
						.and_then(|account| SolAddress::from_str(account).ok()),
					UiMessage::Raw(message) => message
						.account_keys
						.get(0)
						.and_then(|account| SolAddress::from_str(account).ok()),
				}
			} else {
				None
			};

		finalized_txs_info.push((signature, slot, fee, signer));
	}

	finalized_txs_info
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
	#[ignore]
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
