// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0
use crate::{
	dot::cached_rpc::{DotCachingClient, DotRetryRpcApiWithResult},
	witness::{
		common::{
			block_height_witnesser::witness_headers,
			block_witnesser::GenericBwVoter,
			traits::{WitnessClient, WitnessClientForBlockData},
		},
		hub::{
			filter_map_events, hub_deposits::deposit_witnesses, process_egresses_in_block,
			EventWrapper,
		},
	},
};
use cf_chains::{
	assets,
	dot::PolkadotSignature,
	hub::ASSETHUB_WITNESS_PERIOD,
	witness_period::{block_witness_range, block_witness_root, BlockWitnessRange, SaturatingStep},
	Assethub, DepositChannel,
};
use cf_utilities::task_scope::Scope;
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi,
};
use futures::future;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::primitives::Header, ElectoralSystemTypes, VoteOf,
};
use pallet_cf_ingress_egress::DepositWitness;
use state_chain_runtime::{
	chainflip::witnessing::assethub_elections::{
		AssethubBlockHeightWitnesserES, AssethubChain, AssethubElectoralSystemRunner,
		AssethubFeeTracking, AssethubLiveness, AssethubWitnessBatchNumber,
		ASSETHUB_MAINNET_SAFETY_BUFFER, ASSETHUB_MAX_SUBMIT_HEADERS_IN_BHW_VOTER,
	},
	AssethubInstance, Runtime,
};
use std::sync::Arc;
use subxt::events::Phase;

use crate::elections::voter_api::{CompositeVoter, VoterApi};

use anyhow::Result;

#[derive(Clone)]
pub struct AssethubVoter {
	pub client: DotCachingClient,
}

/// Almost all assethub witnessing tasks need access to a block hash
/// and list of events in a block, so it makes sense to combine them
/// into this struct. This is *not* the format of an official substrate header.
#[derive(Debug, Clone)]
pub struct AssethubBlockHeader {
	pub block_hash: sp_core::H256,
	pub parent_block_hash: sp_core::H256,
	pub block_height: u32,
	pub parsed_events: Vec<(Phase, EventWrapper)>,
}

#[async_trait::async_trait]
impl WitnessClient<AssethubChain> for AssethubVoter {
	type BlockQuery = Vec<AssethubBlockHeader>;

	// --- BHW methods ---

	async fn best_block_header(&self) -> Result<Header<AssethubChain>> {
		self.block_header_by_height(self.best_block_number().await?).await
	}

	async fn block_header_by_height(
		&self,
		height: AssethubWitnessBatchNumber,
	) -> Result<Header<AssethubChain>> {
		let finalized_height = self.best_block_number().await?;
		if height > finalized_height {
			Err(anyhow::anyhow!(
				"Block height {height:?} is not yet finalized (highest finalized {finalized_height:?})"
			))
		} else {
			Ok(Header {
				block_height: height.clone(),
				// assethub blocks are identified by block height (i.e BlockWitnessRange)
				hash: height.clone(),
				// this means the parent "hash" is just the previous witness range
				parent_hash: height.saturating_backward(1),
			})
		}
	}

	/// this returns the highest *finalized* block
	async fn best_block_number(&self) -> Result<AssethubWitnessBatchNumber> {
		// ------------------------------------------------------------------- //
		// WARNING: It is *VERY* important that this is a FINALIZED block hash //
		// ------------------------------------------------------------------- //
		let finalized_hash = self.client.finalized_head().await?;
		let Some(finalized_header) = self.client.header(finalized_hash).await? else {
			return Err(anyhow::anyhow!("Got None as header for hash {finalized_hash}."))
		};
		let finalized_height = finalized_header.number;
		let range = block_witness_range(ASSETHUB_WITNESS_PERIOD, finalized_height);
		let block_witness_range = BlockWitnessRange::try_new(block_witness_root(
			ASSETHUB_WITNESS_PERIOD,
			finalized_height,
		))
		.map_err(|_| anyhow::anyhow!("Failed to build BlockWitnessRange"))?;
		if finalized_height == *range.end() {
			return Ok(block_witness_range);
		}
		Ok(block_witness_range.saturating_backward(1))
	}

	// --- BW methods ---

	async fn block_query_from_hash_and_height(
		&self,
		// the hash is just a block height since we only witness finalized blocks
		hash: AssethubWitnessBatchNumber,
		height: AssethubWitnessBatchNumber,
	) -> Result<Self::BlockQuery> {
		if hash == height {
			self.block_query_from_height(height).await
		} else {
			Err(anyhow::anyhow!(
				"Encountered hash != height when creating block query to vote for assethub. {hash:?} != {height:?}"
			))
		}
	}

	async fn block_query_from_height(
		&self,
		witness_range: AssethubWitnessBatchNumber,
	) -> Result<Self::BlockQuery> {
		let block_headers = future::join_all(witness_range.into_range_inclusive().map(
			|finalized_block_height: u64| async move {
				let finalized_block_height: u32 = finalized_block_height
					.try_into()
					.map_err(|_| anyhow::anyhow!("block height doesn't fit into u32!"))?;

				// get events of this block
				let Some(block_hash) = self.client.block_hash(finalized_block_height).await? else {
					return Err(anyhow::anyhow!(
						"No blockhash for block height {finalized_block_height}"
					));
				};
				let Some(header) = self.client.header(block_hash).await? else {
					return Err(anyhow::anyhow!("No header for block hash {block_hash}"));
				};
				let Some(events) = self.client.events(block_hash, header.parent_hash).await? else {
					return Err(anyhow::anyhow!("No events for block hash {block_hash}"));
				};

				let parsed_events =
					events.iter().filter_map(crate::witness::hub::filter_map_events).collect();

				Ok(AssethubBlockHeader {
					block_height: finalized_block_height,
					block_hash,
					parent_block_hash: header.parent_hash,
					parsed_events,
				})
			},
		))
		.await;

		// This only succeeds if *all* futures were successful
		let block_headers: Vec<_> = block_headers.into_iter().collect::<anyhow::Result<_>>()?;

		Ok(block_headers)
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: AssethubWitnessBatchNumber,
	) -> Result<(Self::BlockQuery, AssethubWitnessBatchNumber)> {
		Ok((self.block_query_from_height(height.clone()).await?, height))
	}
}

#[async_trait::async_trait]
impl VoterApi<AssethubBlockHeightWitnesserES> for AssethubVoter {
	async fn vote(
		&self,
		_settings: <AssethubBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <AssethubBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<AssethubBlockHeightWitnesserES>>> {
		witness_headers::<AssethubBlockHeightWitnesserES, _, AssethubChain>(
			self,
			properties,
			ASSETHUB_MAINNET_SAFETY_BUFFER,
			ASSETHUB_MAX_SUBMIT_HEADERS_IN_BHW_VOTER,
			"Assethub BHW",
		)
		.await
	}
}

#[async_trait::async_trait]
impl WitnessClientForBlockData<AssethubChain, Vec<DepositWitness<Assethub>>> for AssethubVoter {
	type ElectionProperties = Vec<DepositChannel<Assethub>>;

	async fn block_data_from_query(
		&self,
		_config: &Self::Config,
		deposit_channels: &Self::ElectionProperties,
		block_headers: &Self::BlockQuery,
	) -> Result<Vec<DepositWitness<Assethub>>> {
		let results = future::join_all(block_headers.into_iter().map(|header| async move {
			// compute deposit witneses
			let addresses = deposit_channels
				.into_iter()
				.map(|deposit_channel| {
					assert!(
						deposit_channel.asset == assets::hub::Asset::HubDot ||
							deposit_channel.asset == assets::hub::Asset::HubUsdc ||
							deposit_channel.asset == assets::hub::Asset::HubUsdt
					);
					deposit_channel.address
				})
				.collect();

			let deposit_witnesses = deposit_witnesses(
				header.block_hash,
				Some(header.parent_block_hash),
				&self.client,
				addresses,
				&header.parsed_events,
			)
			.await?;

			Ok(deposit_witnesses)
		}))
		.await;

		// This converts a vector of results into a result with a vector
		// I.e., if one of the requests failed, we don't submit anything
		let successes: Vec<Vec<DepositWitness<Assethub>>> =
			results.into_iter().collect::<anyhow::Result<Vec<_>>>()?;

		let mut deposit_witnesses: Vec<DepositWitness<Assethub>> =
			successes.into_iter().flatten().collect();

		// Ensure that the vote is deterministic and doesn't depend on accidental ordering
		deposit_witnesses.sort();

		Ok(deposit_witnesses)
	}
}

#[async_trait::async_trait]
impl
	WitnessClientForBlockData<
		AssethubChain,
		Vec<TransactionConfirmation<Runtime, AssethubInstance>>,
	> for AssethubVoter
{
	type ElectionProperties = Vec<PolkadotSignature>;

	async fn block_data_from_query(
		&self,
		_config: &Self::Config,
		pending_tx_signatures: &Self::ElectionProperties,
		block_headers: &Self::BlockQuery,
	) -> Result<Vec<TransactionConfirmation<Runtime, AssethubInstance>>> {
		let results = future::join_all(block_headers.into_iter().map(|header| async move {
			process_egresses_in_block(&self.client, pending_tx_signatures, header).await
		}))
		.await;

		// This converts a vector of results into a result with a vector
		// I.e., if one of the requests failed, we don't submit anything
		let successes: Vec<Vec<_>> = results.into_iter().collect::<anyhow::Result<Vec<_>>>()?;

		let mut egress_witnesses: Vec<_> = successes.into_iter().flatten().collect();

		// Ensure that the vote is deterministic and doesn't depend on accidental ordering
		egress_witnesses.sort();

		Ok(egress_witnesses)
	}
}

#[derive(Clone)]
pub struct AssethubFeeVoter {
	_client: DotCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<AssethubFeeTracking> for AssethubFeeVoter {
	async fn vote(
		&self,
		_settings: <AssethubFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <AssethubFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<AssethubFeeTracking>>> {
		Err(anyhow::anyhow!("Fee voter not implemented"))
	}
}

#[derive(Clone)]
pub struct AssethubLivenessVoter {
	_client: DotCachingClient,
}

#[async_trait::async_trait]
impl VoterApi<AssethubLiveness> for AssethubLivenessVoter {
	async fn vote(
		&self,
		_settings: <AssethubLiveness as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <AssethubLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<AssethubLiveness>>> {
		Err(anyhow::anyhow!("Liveness voter not implemented"))
	}
}

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: DotCachingClient,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<AssethubInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting Assethub election witness");

	crate::elections::Voter::new(
		scope,
		state_chain_client,
		CompositeVoter::<AssethubElectoralSystemRunner, _>::new((
			AssethubVoter { client: client.clone() },
			GenericBwVoter::new(AssethubVoter { client: client.clone() }, ()),
			GenericBwVoter::new(AssethubVoter { client: client.clone() }, ()),
			AssethubFeeVoter { _client: client.clone() },
			AssethubLivenessVoter { _client: client.clone() },
		)),
		Some(client.cache_invalidation_senders),
		"Assethub",
	)
	.continuously_vote()
	.await;

	Ok(())
}
