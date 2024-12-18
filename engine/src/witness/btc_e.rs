//! For BTC Elections

use bitcoin::hashes::Hash;
use cf_chains::{
	btc::{self, BlockNumber},
	witness_period::BlockWitnessRange,
	Bitcoin,
};
use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::block_height_tracking::{primitives::Header, BlockHeightTrackingProperties},
	vote_storage::VoteStorage,
};
use sp_core::bounded::alloc::collections::VecDeque;
use state_chain_runtime::{
	chainflip::bitcoin_elections::{
		BitcoinBlockHeightTracking, BitcoinDepositChannelWitnessing, BitcoinElectoralSystemRunner,
	},
	BitcoinInstance,
};

use crate::{
	btc::{retry_rpc::BtcRetryRpcApi, rpc::BlockHeader},
	elections::voter_api::{CompositeVoter, VoterApi},
	retrier::RetryLimit,
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::btc::deposits::{deposit_witnesses, map_script_addresses},
};
use anyhow::{anyhow, Result};

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
		for _block in BlockWitnessRange::<u64>::into_range_inclusive(witness_range) {
			// TODO: these queries should not be infinite
			// let block_hash = self.client.block_hash(block).await;

			let latest_hash = self.client.best_block_header().await?.hash;

			let block = self.client.block(latest_hash).await?;
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

#[derive(Clone)]
pub struct BitcoinBlockHeightTrackingVoter {
	client: BtcRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinBlockHeightTracking> for BitcoinBlockHeightTrackingVoter {
	async fn vote(
		&self,
		_settings: <BitcoinBlockHeightTracking as ElectoralSystem>::ElectoralSettings,
		// We could use 0 as a special case (to avoid requiring an Option)
		properties: <BitcoinBlockHeightTracking as ElectoralSystem>::ElectionProperties,
	) -> std::result::Result<
		<<BitcoinBlockHeightTracking as ElectoralSystem>::Vote as VoteStorage>::Vote,
		anyhow::Error,
	> {
		tracing::info!("Block height tracking called properties: {:?}", properties);
		let BlockHeightTrackingProperties { witness_from_index } = properties;

		let witness_from_index = witness_from_index.root().clone();

		let mut headers = VecDeque::new();

		let header_from_btc_header = |header: BlockHeader| -> anyhow::Result<
			Header<btc::Hash, BlockWitnessRange<btc::BlockNumber>>,
		> {
			Ok(Header {
				block_height: BlockWitnessRange::try_new(header.height, 1)
					.map_err(|_| anyhow!(""))?,
				hash: header.hash.to_byte_array().into(),
				parent_hash: header
					.previous_block_hash
					.ok_or_else(|| anyhow::anyhow!("No parent hash"))?
					.to_byte_array()
					.into(),
			})
		};

		let get_header = |index: BlockNumber| {
			async move {
				let header = self.client.block_header(index).await?;
				// tracing::info!("bht: Voting for block height tracking: {:?}", header.height);
				// Order from lowest to highest block index.
				Ok::<Header<sp_core::H256, BlockWitnessRange<u64>>, anyhow::Error>(
					header_from_btc_header(header)?,
				)
			}
		};

		let best_block_header = header_from_btc_header(self.client.best_block_header().await?)?;

		if *best_block_header.block_height.root() <= witness_from_index {
			Err(anyhow::anyhow!("btc: no new blocks found since best block height is {} for witness_from={witness_from_index}", best_block_header.block_height.root()))
		} else if witness_from_index == 0 {
			headers.push_back(best_block_header);
			Ok(headers)
		} else {
			// fetch the headers we haven't got yet
			for index in witness_from_index..*best_block_header.block_height.root() {
				// let header = self.client.block_header(index).await?;
				// tracing::info!("bht: Voting for block height tracking: {:?}", header.height);
				// Order from lowest to highest block index.
				headers.push_back(get_header(index).await?);
			}

			headers.push_back(best_block_header);
			tracing::info!(
				"bht: Voting for block height tracking: {:?}",
				headers.iter().map(|h| h.block_height)
			);

			// We should have a chain of hashees.
			if headers.iter().zip(headers.iter().skip(1)).all(|(a, b)| a.hash == b.parent_hash) {
				tracing::info!(
					"bht: Submitting vote for (witness_from={witness_from_index})with {} headers",
					headers.len()
				);
				Ok(headers)
			} else {
				Err(anyhow::anyhow!("bht: Headers do not form a chain"))
			}
		}
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
	tracing::info!("Starting BTC witness");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<BitcoinElectoralSystemRunner, _>::new((
						BitcoinBlockHeightTrackingVoter { client: client.clone() },
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
