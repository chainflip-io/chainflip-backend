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

use std::{future::Future, time::Duration};

use bitcoin::BlockHash;
use cf_chains::btc;
use cf_utilities::make_periodic_tick;
use futures_util::stream;
use tokio::time::sleep;

use crate::{
	btc::rpc::BtcRpcApi,
	witness::common::{
		chain_source::{BoxChainStream, ChainClient, ChainSource, Header},
		ExternalChainSource,
	},
};

#[derive(Clone)]
pub struct BtcSource<C> {
	client: C,
}

impl<C> BtcSource<C> {
	pub fn new(client: C) -> Self {
		Self { client }
	}
}

const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct BtcSourceState {
	last_block_yielded_hash: BlockHash,
	last_block_yielded_index: btc::BlockNumber,
	best_known_block_index: btc::BlockNumber,
}

#[async_trait::async_trait]
impl<C> ChainSource for BtcSource<C>
where
	C: Clone + BtcRpcApi + ChainClient<Index = u64, Hash = BlockHash, Data = ()>,
{
	type Index = <C as ChainClient>::Index;
	type Hash = <C as ChainClient>::Hash;
	type Data = <C as ChainClient>::Data;
	type Client = C;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client) {
		(
			Box::pin(stream::unfold(
				(
					self.client.clone(),
					Option::<BtcSourceState>::None,
					make_periodic_tick(POLL_INTERVAL, true),
				),
				|(client, mut stream_state, mut tick)| async move {
					loop {
						// We don't want to wait for the tick if we have backfilling to do, so we do
						// it here before awaiting the tick.
						if let Some(state) = &stream_state {
							if state.best_known_block_index > state.last_block_yielded_index {
								tracing::debug!(
									"Backfilling BTC source from index {} to {}",
									state.last_block_yielded_index,
									state.best_known_block_index,
								);
								let header = client
									.header_at_index(
										state.last_block_yielded_index.saturating_add(1),
									)
									.await;
								return Some((
									header,
									(
										client,
										Some(BtcSourceState {
											last_block_yielded_hash: header.hash,
											last_block_yielded_index: header.index,
											best_known_block_index: state.best_known_block_index,
										}),
										tick,
									),
								));
							}
						}

						tick.tick().await;

						async fn until_success<A, Fut: Future<Output = anyhow::Result<A>>>(
							f: impl Fn() -> Fut,
						) -> A {
							loop {
								match f().await {
									Ok(a) => return a,
									Err(err) => tracing::warn!("Received error {err} when trying to query btc rpc. Retrying."),
								}
								sleep(Duration::from_secs(6)).await;
							}
						}

						let client_c = client.clone();
						let best_block_header = until_success(move || {
							let client_c = client_c.clone();
							async move {
								let best_block_hash = client_c.best_block_hash().await?;
								let best_block_header =
									client_c.block_header(best_block_hash).await?;
								Ok(best_block_header)
							}
						})
						.await;

						let yield_new_best_header: bool = match &mut stream_state {
							Some(state)
								// We want to immediately yield the new best header if we've reorged on the same block
								// or it's the next block we expect
								if (state.last_block_yielded_index == best_block_header.height &&
									state.last_block_yielded_hash != best_block_header.hash) ||
									state.last_block_yielded_index.saturating_add(1) == best_block_header.height =>
								true,
							// If we don't yet have state (we're initialising), then we want to
							// yield the new best header immediately
							None => true,
							// If we've progressed by more than one block, then we need to backfill
							Some(state)
								if state.last_block_yielded_index < best_block_header.height =>
							{
								// Update the state for the next iteration to backfill
								state.best_known_block_index = best_block_header.height;
								false
							},
							// do nothing, just loop again.
							_ => false,
						};

						if yield_new_best_header {
							// Yield the new best header immediately
							return Some((
								Header {
									index: best_block_header.height,
									hash: best_block_header.hash,
									parent_hash: best_block_header.previous_block_hash,
									data: (),
								},
								(
									client,
									Some(BtcSourceState {
										last_block_yielded_hash: best_block_header.hash,
										last_block_yielded_index: best_block_header.height,
										best_known_block_index: best_block_header.height,
									}),
									tick,
								),
							));
						}
					}
				},
			)),
			self.client.clone(),
		)
	}
}

impl<C> ExternalChainSource for BtcSource<C>
where
	C: BtcRpcApi + ChainClient<Index = u64, Hash = BlockHash, Data = ()> + Clone,
{
	type Chain = cf_chains::Bitcoin;
}
