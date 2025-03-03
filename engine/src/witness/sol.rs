mod egress_witnessing;
mod lookup_table_witnessing;
mod nonce_witnessing;
mod sol_deposits;
mod vault_swaps_witnessing;

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::{RpcBlockConfig, TransactionDetails},
	},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};
use anyhow::Result;
use cf_chains::{
	sol::{api::VaultSwapAccountAndSender, SolHash},
	Chain,
};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_systems::solana_vault_swap_accounts::SolanaVaultSwapsVote, ElectoralSystemTypes,
	VoteOf,
};
use state_chain_runtime::{
	chainflip::solana_elections::{
		SolanaAltWitnessing, SolanaBlockHeightTracking, SolanaEgressWitnessing,
		SolanaElectoralSystemRunner, SolanaIngressTracking, SolanaLiveness, SolanaNonceTracking,
		SolanaVaultSwapTracking, TransactionSuccessDetails,
	},
	SolanaInstance,
};

use cf_utilities::{metrics::CHAIN_TRACKING, task_scope, task_scope::Scope};
use pallet_cf_elections::vote_storage::change::MonotonicChangeVote;
use std::{
	collections::{BTreeSet, HashSet},
	str::FromStr,
	sync::Arc,
};

#[derive(Clone)]
struct SolanaBlockHeightTrackingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaBlockHeightTracking> for SolanaBlockHeightTrackingVoter {
	async fn vote(
		&self,
		_settings: <SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaBlockHeightTracking>>, anyhow::Error> {
		let slot = self.client.get_slot(CommitmentConfig::finalized()).await;
		CHAIN_TRACKING.set(&[cf_chains::Solana::NAME], Into::<u64>::into(slot));
		Ok(Some(slot))
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
		settings: <SolanaIngressTracking as ElectoralSystemTypes>::ElectoralSettings,
		properties: <SolanaIngressTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaIngressTracking>>, anyhow::Error> {
		sol_deposits::get_channel_ingress_amounts(
			&self.client,
			settings.0.vault_program,
			settings.0.usdc_token_mint_pubkey,
			properties.0,
		)
		.await
		.and_then(|vote| {
			vote.try_into().map_err(|_| anyhow::anyhow!("Too many channels in election"))
		})
		.map(Option::Some)
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
		_settings: <SolanaNonceTracking as ElectoralSystemTypes>::ElectoralSettings,
		properties: <SolanaNonceTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaNonceTracking>>, anyhow::Error> {
		let (nonce_account, previous_nonce, previous_slot) = properties;

		let nonce_and_slot =
			nonce_witnessing::get_durable_nonce(&self.client, nonce_account, previous_slot)
				.await?
				.map(|(nonce, slot)| MonotonicChangeVote { value: nonce, block: slot });
		// If the nonce is not found, we default to the previous nonce and slot.
		// The `MonotonicChange` electoral system ensure this vote is filtered.
		Ok(Some(
			nonce_and_slot
				.unwrap_or(MonotonicChangeVote { value: previous_nonce, block: previous_slot }),
		))
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
		_settings: <SolanaEgressWitnessing as ElectoralSystemTypes>::ElectoralSettings,
		signature: <SolanaEgressWitnessing as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaEgressWitnessing>>, anyhow::Error> {
		egress_witnessing::get_finalized_fee_and_success_status(&self.client, signature)
			.await
			.map(|(tx_fee, transaction_successful)| TransactionSuccessDetails {
				tx_fee,
				transaction_successful,
			})
			.map(Option::Some)
	}
}

#[derive(Clone)]
struct SolanaLivenessVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaLiveness> for SolanaLivenessVoter {
	async fn vote(
		&self,
		_settings: <SolanaLiveness as ElectoralSystemTypes>::ElectoralSettings,
		slot: <SolanaLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaLiveness>>, anyhow::Error> {
		Ok(Some(
			SolHash::from_str(
				&self
					.client
					.get_block(
						slot,
						RpcBlockConfig {
							transaction_details: Some(TransactionDetails::None),
							rewards: Some(false),
							max_supported_transaction_version: Some(0),
							..Default::default()
						},
					)
					.await
					.blockhash,
			)
			.map_err(|e| anyhow::anyhow!("Failed to convert blockhash String to SolHash: {e}"))?,
		))
	}
}

#[derive(Clone)]
struct SolanaVaultSwapsVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaVaultSwapTracking> for SolanaVaultSwapsVoter {
	async fn vote(
		&self,
		settings: <SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectoralSettings,
		properties: <SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaVaultSwapTracking>>, anyhow::Error> {
		vault_swaps_witnessing::get_vault_swaps(
			&self.client,
			settings.swap_endpoint_data_account_address,
			properties
				.witnessed_open_accounts
				.into_iter()
				.map(|(VaultSwapAccountAndSender { vault_swap_account, .. }, _)| vault_swap_account)
				.collect::<HashSet<_>>(),
			properties.closure_initiated_accounts,
			settings.usdc_token_mint_pubkey,
		)
		.await
		.map(|(new_accounts, confirm_closed_accounts)| SolanaVaultSwapsVote {
			new_accounts: new_accounts.into_iter().collect::<BTreeSet<_>>(),
			confirm_closed_accounts: confirm_closed_accounts.into_iter().collect::<BTreeSet<_>>(),
		})
		.map(Option::Some)
	}
}

#[allow(dead_code)]
#[derive(Clone)]
struct SolanaAltWitnessingVoter {
	client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<SolanaAltWitnessing> for SolanaAltWitnessingVoter {
	async fn vote(
		&self,
		_settings: <SolanaAltWitnessing as ElectoralSystemTypes>::ElectoralSettings,
		alt_witnessing_identifier: <SolanaAltWitnessing as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<SolanaAltWitnessing>>, anyhow::Error> {
		lookup_table_witnessing::get_lookup_table_state(
			&self.client,
			alt_witnessing_identifier.alt_addresses,
		)
		.await
		// We wrap the vote in a Some her since the vote is always valid if there was no error in
		// rpc while querying.
		.map(|maybe_lookup_tables| Some(maybe_lookup_tables))
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
					CompositeVoter::<SolanaElectoralSystemRunner, _>::new((
						SolanaBlockHeightTrackingVoter { client: client.clone() },
						SolanaIngressTrackingVoter { client: client.clone() },
						SolanaNonceTrackingVoter { client: client.clone() },
						SolanaEgressWitnessingVoter { client: client.clone() },
						SolanaLivenessVoter { client: client.clone() },
						SolanaVaultSwapsVoter { client: client.clone() },
						SolanaAltWitnessingVoter { client },
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
