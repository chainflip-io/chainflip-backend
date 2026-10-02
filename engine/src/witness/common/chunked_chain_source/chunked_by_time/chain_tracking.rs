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

use std::sync::Arc;

use cf_chains::{instances::ChainInstanceFor, ChainState};

use crate::witness::common::{chain_source::Header, RuntimeCallHasChain, RuntimeHasChain};
use cf_chains::Chain;
use cf_utilities::metrics::CHAIN_TRACKING;
use engine_sc_client::{
	chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	STATE_CHAIN_CONNECTION,
};

use super::{builder::ChunkedByTimeBuilder, ChunkedByTime};

#[async_trait::async_trait]
pub trait GetTrackedData<C: cf_chains::Chain, Hash, Data>: Send + Sync + Clone {
	async fn get_tracked_data(
		&self,
		header: &Header<C::ChainBlockNumber, Hash, Data>,
	) -> Result<C::TrackedData, anyhow::Error>;

	/// Whether to witness chain tracking at this height. Must be deterministic so that all
	/// validators vote on the same heights.
	fn should_witness(_index: C::ChainBlockNumber) -> bool {
		true
	}

	/// Whether this height is too far behind the State Chain's tracked height for a vote to
	/// matter: it can neither advance chain tracking nor arrive within the late-witness grace
	/// period.
	fn is_stale(_index: C::ChainBlockNumber, _tracked_height: C::ChainBlockNumber) -> bool {
		false
	}
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub fn chain_tracking<StateChainClient, TrackedDataClient>(
		self,
		state_chain_client: Arc<StateChainClient>,
		tracked_data_client: TrackedDataClient,
	) -> ChunkedByTimeBuilder<impl ChunkedByTime>
	where
		Inner: ChunkedByTime,
		StateChainClient: ChainApi + StorageApi + SignedExtrinsicApi + Send + Sync + 'static,
		TrackedDataClient: GetTrackedData<Inner::Chain, Inner::Hash, Inner::Data>,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		// Witness every header selected by `should_witness` rather than only the latest: skipping
		// depends on local timing, so validators would otherwise vote on different heights. This
		// is cheap because `finalize_signed_extrinsic` only queues the extrinsic; we don't await
		// `until_finalized`.
		self.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let tracked_data_client = tracked_data_client.clone();
			async move {
				if !TrackedDataClient::should_witness(header.index) {
					return Ok(header.data)
				}
				if let Some(tracked) = state_chain_client
					.storage_value::<pallet_cf_chain_tracking::CurrentChainState<
						state_chain_runtime::Runtime,
						ChainInstanceFor<Inner::Chain>,
					>>(state_chain_client.latest_finalized_block().hash)
					.await
					.expect(STATE_CHAIN_CONNECTION)
				{
					if TrackedDataClient::is_stale(header.index, tracked.block_height) {
						return Ok(header.data)
					}
				}
				let call: Box<state_chain_runtime::RuntimeCall> = Box::new(
					pallet_cf_chain_tracking::Call::<
						state_chain_runtime::Runtime,
						ChainInstanceFor<Inner::Chain>,
					>::update_chain_state {
						new_chain_state: ChainState {
							block_height: header.index,
							tracked_data: tracked_data_client.get_tracked_data(&header).await?,
						},
					}
					.into(),
				);
				state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call,
						epoch_index: epoch.index,
					})
					.await;
				CHAIN_TRACKING.set(&[Inner::Chain::NAME], Into::<u64>::into(header.index));
				Ok::<_, anyhow::Error>(header.data)
			}
		})
	}
}
