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

use cf_chains::instances::{ChainInstanceAlias, ChainInstanceFor};
use chainflip_api::primitives::BroadcastId;
use engine_sc_client::{chain_api::ChainApi, storage_api::StorageApi, STATE_CHAIN_CONNECTION};
use pallet_cf_broadcast::TransactionOutIdFor;
use sp_core::bounded::alloc::sync::Arc;

use tracing::log;

pub async fn get_broadcast_id<I, StateChainClient>(
	state_chain_client: Arc<StateChainClient>,
	tx_out_id: &TransactionOutIdFor<state_chain_runtime::Runtime, ChainInstanceFor<I>>,
) -> Option<BroadcastId>
where
	state_chain_runtime::Runtime: pallet_cf_broadcast::Config<ChainInstanceFor<I>>,
	I: ChainInstanceAlias + 'static,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
{
	let id = state_chain_client
		.storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
			state_chain_runtime::Runtime,
			ChainInstanceFor<I>,
		>>(state_chain_client.latest_unfinalized_block().hash, tx_out_id)
		.await
		.expect(STATE_CHAIN_CONNECTION)
		.map(|(broadcast_id, _)| broadcast_id);

	if id.is_none() {
		log::warn!("Broadcast ID not found for {:?}", tx_out_id);
	}

	id
}
