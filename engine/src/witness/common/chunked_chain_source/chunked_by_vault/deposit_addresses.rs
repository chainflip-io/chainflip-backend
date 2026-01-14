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

use cf_chains::{instances::ChainInstanceFor, Chain};
use cf_utilities::task_scope::Scope;
use pallet_cf_ingress_egress::DepositChannelDetails;
use std::sync::Arc;

use crate::witness::common::RuntimeHasChain;
use engine_sc_client::{storage_api::StorageApi, stream_api::StreamApi, STATE_CHAIN_CONNECTION};

use super::{builder::ChunkedByVaultBuilder, monitored_items::MonitoredSCItems, ChunkedByVault};

pub type Addresses<Inner> = Vec<
	DepositChannelDetails<
		state_chain_runtime::Runtime,
		ChainInstanceFor<<Inner as ChunkedByVault>::Chain>,
	>,
>;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn deposit_addresses<
		'env,
		StateChainStream,
		StateChainClient,
		const IS_FINALIZED: bool,
	>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> ChunkedByVaultBuilder<
		MonitoredSCItems<
			Inner,
			Addresses<Inner>,
			impl Fn(<Inner::Chain as Chain>::ChainBlockNumber, &Addresses<Inner>) -> Addresses<Inner>
				+ Send
				+ Sync
				+ Clone
				+ 'static,
		>,
	>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StreamApi<IS_FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
	{
		let state_chain_client_c = state_chain_client.clone();
		ChunkedByVaultBuilder::new(
			MonitoredSCItems::new(
				self.source,
				scope,
				state_chain_stream,
				state_chain_client,
				move |block_hash| {
					let state_chain_client = state_chain_client_c.clone();
					async move {
						state_chain_client
							.storage_map_values::<pallet_cf_ingress_egress::DepositChannelLookup<
								state_chain_runtime::Runtime,
								ChainInstanceFor<Inner::Chain>,
							>>(block_hash)
							.await
							.expect(STATE_CHAIN_CONNECTION)
					}
				},
				|index, addresses: &Addresses<Inner>| {
					assert!(<Inner::Chain as Chain>::is_block_witness_root(index));
					addresses
						.iter()
						.filter(|details| {
							assert!(<Inner::Chain as Chain>::is_block_witness_root(
								details.opened_at
							));
							assert!(<Inner::Chain as Chain>::is_block_witness_root(
								details.expires_at
							));
							details.opened_at <= index && index <= details.expires_at
						})
						.cloned()
						.collect()
				},
			)
			.await,
			self.parameters,
		)
	}
}
