//! For BTC Elections

use cf_chains::witness_period::BlockWitnessRange;
use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{electoral_system::ElectoralSystem, vote_storage::VoteStorage};
use state_chain_runtime::{
	chainflip::bitcoin_elections::{BitcoinDepositChannelWitnessing, BitcoinElectoralSystemRunner},
	BitcoinInstance,
};

use crate::{
	btc::retry_rpc::BtcRetryRpcApi,
	elections::voter_api::{CompositeVoter, VoterApi},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::btc::deposits::{deposit_witnesses, map_script_addresses},
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
		deposit_addresses: <BitcoinDepositChannelWitnessing as ElectoralSystem>::ElectionProperties,
	) -> Result<
		<<BitcoinDepositChannelWitnessing as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		let (witness_range, deposit_addresses) = deposit_addresses;
		tracing::info!("Deposit channel witnessing properties: {:?}", deposit_addresses);

		let mut txs = vec![];
		// we only ever expect this to be one for bitcoin, but for completeness, we loop.
		for block in BlockWitnessRange::<u64>::into_range_inclusive(witness_range) {
			// TODO: these queries should not be infinite
			// let block_hash = self.client.block_hash(block).await;

			let latest_hash = self.client.best_block_header().await.hash;

			let block = self.client.block(latest_hash).await;
			txs.extend(block.txdata);
		}

		let deposit_addresses = map_script_addresses(deposit_addresses);

		let witnesses = deposit_witnesses(&txs, &deposit_addresses);

		if witnesses.is_empty() {
			tracing::info!("No witnesses found for BTCE");
		} else {
			tracing::info!("Witnesses from BTCE: {:?}", witnesses);
		}

		Ok(witnesses)
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
		+ ElectoralApi<cf_chains::Bitcoin, BitcoinInstance>
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
