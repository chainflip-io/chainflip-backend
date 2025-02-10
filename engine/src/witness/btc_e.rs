//! For BTC Elections

use bitcoin::hashes::Hash;
use cf_chains::{
	btc::{self, BlockNumber},
	witness_period::BlockWitnessRange,
};
use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::{
		block_height_tracking::{
			primitives::Header, state_machine::InputHeaders, BlockHeightTrackingProperties,
			BlockHeightTrackingTypes,
		},
		state_machine::core::ConstantIndex,
	},
	VoteOf,
};
use sp_core::bounded::alloc::collections::VecDeque;
use state_chain_runtime::{
	chainflip::bitcoin_elections::{
		BitcoinBlockHeightTracking, BitcoinBlockHeightTrackingTypes,
		BitcoinDepositChannelWitnessing, BitcoinElectoralSystemRunner, BitcoinLiveness,
	},
	BitcoinInstance,
};

use crate::{
	btc::{retry_rpc::BtcRetryRpcApi, rpc::BlockHeader},
	elections::voter_api::{CompositeVoter, VoterApi},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::btc::deposits::{deposit_witnesses, map_script_addresses},
};
use anyhow::Result;

use sp_core::H256;
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
		_settings: <BitcoinDepositChannelWitnessing as ElectoralSystemTypes>::ElectoralSettings,
		deposit_addresses: <BitcoinDepositChannelWitnessing as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinDepositChannelWitnessing>>, anyhow::Error> {
		let (witness_range, deposit_addresses, _extra) = deposit_addresses;
		let witness_range = BlockWitnessRange::try_new(witness_range).unwrap();
		tracing::info!("Deposit channel witnessing properties: {:?}", deposit_addresses);

		let mut txs = vec![];
		// we only ever expect this to be one for bitcoin, but for completeness, we loop.
		tracing::info!("Witness range: {:?}", witness_range);
		for block in BlockWitnessRange::<cf_chains::Bitcoin>::into_range_inclusive(witness_range) {
			tracing::info!("Checking block {:?}", block);

			// TODO: these queries should not be infinite
			let block_hash = self.client.block_hash(block).await;

			let block = self.client.block(block_hash).await?;

			txs.extend(block.txdata);
		}

		let deposit_addresses = map_script_addresses(deposit_addresses);

		let witnesses = deposit_witnesses(&txs, &deposit_addresses);

		if witnesses.is_empty() {
			tracing::info!("No witnesses found for BTCE");
		} else {
			tracing::info!("Witnesses from BTCE: {:?}", witnesses);
		}

		Ok(Some(ConstantIndex { data: witnesses, _phantom: Default::default() }))
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
		_settings: <BitcoinBlockHeightTracking as ElectoralSystemTypes>::ElectoralSettings,
		// We could use 0 as a special case (to avoid requiring an Option)
		properties: <BitcoinBlockHeightTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BitcoinBlockHeightTracking>>, anyhow::Error> {
		tracing::info!("Block height tracking called properties: {:?}", properties);
		let BlockHeightTrackingProperties { witness_from_index: election_property } = properties;

		let mut headers = VecDeque::new();

		let header_from_btc_header =
			|header: BlockHeader| -> anyhow::Result<Header<btc::Hash, btc::BlockNumber>> {
				Ok(Header {
					block_height: header.height,
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
				Ok::<Header<sp_core::H256, u64>, anyhow::Error>(header_from_btc_header(header)?)
			}
		};

		let best_block_header = header_from_btc_header(self.client.best_block_header().await?)?;

		if best_block_header.block_height <= election_property {
			tracing::info!("btc: no new blocks found since best block height is {} for witness_from={election_property}", best_block_header.block_height);
			return Ok(None)
		} else {
			let witness_from_index = if election_property == 0 {
				tracing::info!(
					"bht: election_property=0, best_block_height={}, submitting last 6 blocks.",
					best_block_header.block_height
				);
				best_block_header
					.block_height
					.saturating_sub(BitcoinBlockHeightTrackingTypes::BLOCK_BUFFER_SIZE as u64)
			} else {
				election_property
			};

			// fetch the headers we haven't got yet
			for index in witness_from_index..best_block_header.block_height {
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
					"bht: Submitting vote for (witness_from={election_property})with {} headers",
					headers.len()
				);
				Ok(Some(InputHeaders(headers)))
			} else {
				Err(anyhow::anyhow!("bht: Headers do not form a chain"))
			}
		}
	}
}

#[derive(Clone)]
pub struct BitcoinLivenessVoter {
	client: BtcRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<BitcoinLiveness> for BitcoinLivenessVoter {
	async fn vote(
		&self,
		_settings: <BitcoinLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BitcoinLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<BitcoinLiveness>>, anyhow::Error> {
		Ok(Some(H256::from_slice(&self.client.block_hash(properties).await.to_byte_array())))
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
	tracing::info!("Starting BTC witness");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<BitcoinElectoralSystemRunner, _>::new((
						BitcoinBlockHeightTrackingVoter { client: client.clone() },
						BitcoinDepositChannelWitnessingVoter { client: client.clone() },
						BitcoinLivenessVoter { client },
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
