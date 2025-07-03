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

use cf_primitives::EpochIndex;
use futures_core::Future;
use cf_chains::sol::SolAddress;

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::{
		block_height_witnesser::{
			primitives::{Header, NonemptyContinuousHeaders},
			ChainBlockHashOf, ChainTypes, HeightWitnesserProperties,
		},
		block_witnesser::state_machine::{BWElectionProperties, EngineElectionType},
	},
	VoteOf,
};
use sol_prim::{consts::const_address, Address};
use sp_core::bounded::alloc::collections::VecDeque;
use state_chain_runtime::chainflip::generic_elections::*;

use crate::{
	btc::rpc::BlockHeader,
	elections::voter_api::{CompositeVoter, VoterApi},
	sol::retry_rpc::SolRetryRpcClient,
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::{
		btc::deposits::{deposit_witnesses, map_script_addresses},
		sol::oracle_witnessing::get_price_feeds,
	},
};
use anyhow::{bail, Result};

#[derive(Clone)]
struct OraclePriceVoter {
	sol_client: SolRetryRpcClient,
}

#[async_trait::async_trait]
impl VoterApi<OraclePriceES> for OraclePriceVoter {
	async fn vote(
		&self,
		_settings: <OraclePriceES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <OraclePriceES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<OraclePriceES>>, anyhow::Error> {
		tracing::info!("Voting for oracle price, properties: {properties:?}");

        let oracle_program_id: SolAddress = const_address("DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X");
		let oracle_feeds = vec![const_address("HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG")];
		let oracle_query_helper: SolAddress = const_address("GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z");

		let (price_feeds, query_timestamp, query_slot) = get_price_feeds(
			&self.sol_client,
			oracle_query_helper,
			oracle_program_id,
			oracle_feeds,
			None,
		)
		.await?;

		tracing::info!("Got the following price data: {price_feeds:?}, at time {query_timestamp:?} and slot {query_slot:?}");

		Ok(None)
	}
}

use std::sync::Arc;
pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	sol_client: SolRetryRpcClient,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient:
		StorageApi + ChainApi + SignedExtrinsicApi + ElectoralApi<()> + 'static + Send + Sync,
{
	tracing::info!("Starting generic election witnessing");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<GenericElectoralSystemRunner, _>::new((OraclePriceVoter {
						sol_client: sol_client.clone(),
					},)),
					None,
					"GenericElections",
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
