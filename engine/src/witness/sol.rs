mod egress_witnessing;
mod fee_tracking;
mod nonce_witnessing;
mod sol_deposits;

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	sol::{
		commitment_config::CommitmentConfig,
		retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
		rpc_client_api::RpcBlockConfig,
	},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};
use anyhow::Result;
use cf_chains::sol::SolHash;
use futures::FutureExt;
use pallet_cf_elections::{electoral_system::ElectoralSystem, vote_storage::VoteStorage};
use state_chain_runtime::{
	chainflip::solana_elections::{
		SolanaBlockHeightTracking, SolanaEgressWitnessing, SolanaElectoralSystem,
		SolanaFeeTracking, SolanaIngressTracking, SolanaLiveness, SolanaNonceTracking,
		TransactionSuccessDetails,
	},
	SolanaInstance,
};

use cf_utilities::{task_scope, task_scope::Scope};
use std::{str::FromStr, sync::Arc};

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

// TODO: decide on a reasonable value for this.
const MIN_PRIORITIZATION_FEE: u64 = 0;

#[async_trait::async_trait]
impl VoterApi<SolanaFeeTracking> for SolanaFeeTrackingVoter {
	async fn vote(
		&self,
		_settings: <SolanaFeeTracking as ElectoralSystem>::ElectoralSettings,
		_properties: <SolanaFeeTracking as ElectoralSystem>::ElectionProperties,
	) -> Result<<<SolanaFeeTracking as ElectoralSystem>::Vote as VoteStorage>::Vote, anyhow::Error>
	{
		Ok(fee_tracking::get_median_prioritization_fee(&self.client)
			.await
			.unwrap_or(MIN_PRIORITIZATION_FEE))
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
		sol_deposits::get_channel_ingress_amounts(
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
		let (nonce_account, previous_nonce) = properties;
		Ok(nonce_witnessing::get_durable_nonce(&self.client, nonce_account)
			.await?
			// If the nonce is not found, we default to the previous nonce.
			// The `Change` electoral system ensure this vote is filtered.
			.unwrap_or(previous_nonce))
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
		egress_witnessing::get_finalized_fee_and_success_status(&self.client, signature)
			.await
			.map(|(tx_fee, transaction_successful)| TransactionSuccessDetails {
				tx_fee,
				transaction_successful,
			})
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
		_settings: <SolanaLiveness as ElectoralSystem>::ElectoralSettings,
		slot: <SolanaLiveness as ElectoralSystem>::ElectionProperties,
	) -> Result<<<SolanaLiveness as ElectoralSystem>::Vote as VoteStorage>::Vote, anyhow::Error> {
		Ok(SolHash::from_str(
			&self.client.get_block(slot, RpcBlockConfig::default()).await.blockhash,
		)
		.map_err(|e| anyhow::anyhow!("Failed to convert blockhash String to SolHash: {e}"))?)
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
						SolanaEgressWitnessingVoter { client: client.clone() },
						SolanaLivenessVoter { client },
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
