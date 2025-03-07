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

use super::{builder::ChunkedByVaultBuilder, monitored_items::MonitoredSCItems, ChunkedByVault};
use cf_chains::Chain;
use cf_primitives::{AccountId, ChannelId};
use cf_utilities::task_scope::Scope;
use std::sync::Arc;

pub type BrokerPrivateChannels = Vec<(AccountId, ChannelId)>;

use crate::{
	state_chain_observer::client::{
		storage_api::StorageApi, stream_api::StreamApi, STATE_CHAIN_CONNECTION,
	},
	witness::common::RuntimeHasChain,
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn private_deposit_channels<
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
			BrokerPrivateChannels,
			impl Fn(
					<Inner::Chain as Chain>::ChainBlockNumber,
					&BrokerPrivateChannels,
				) -> BrokerPrivateChannels
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
							.storage_map::<pallet_cf_swapping::BrokerPrivateBtcChannels<
								state_chain_runtime::Runtime,
							>, Vec<_>>(block_hash)
							.await
							.expect(STATE_CHAIN_CONNECTION)
					}
				},
				// Private channels are not reusable (at least at the moment), so we
				// don't need to check for their expiration:
				|index, addresses: &BrokerPrivateChannels| {
					assert!(<Inner::Chain as Chain>::is_block_witness_root(index));
					addresses.clone()
				},
			)
			.await,
			self.parameters,
		)
	}
}
