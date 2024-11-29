//! For BTC Elections

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{electoral_system::ElectoralSystem, vote_storage::VoteStorage};
use state_chain_runtime::{
	chainflip::bitcoin_elections::{BitcoinDepositChannelWitnessing, BitcoinElectoralSystemRunner},
	BitcoinInstance,
};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};
use anyhow::Result;

use std::sync::Arc;

use crate::btc::retry_rpc::BtcRetryRpcClient;

#[derive(Clone)]
pub struct BitcoinDepositChannelWitnessingVoter {
	client: BtcRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinDepositChannelWitnessing> for BitcoinDepositChannelWitnessingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinDepositChannelWitnessing as ElectoralSystem>::ElectoralSettings,
		properties: <BitcoinDepositChannelWitnessing as ElectoralSystem>::ElectionProperties,
	) -> Result<
		<<BitcoinDepositChannelWitnessing as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		tracing::info!("Deposit channel witnessing properties: {:?}", properties);
		Err(anyhow::anyhow!("Not implemented"))
	}
}

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: BtcRetryRpcClient,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<BitcoinInstance>
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
					CompositeVoter::<BitcoinElectoralSystemRunner, _>::new((
						BitcoinDepositChannelWitnessingVoter { client },
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
