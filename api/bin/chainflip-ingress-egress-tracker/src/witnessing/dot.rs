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

use super::EnvironmentParameters;
use crate::DepositTrackerSettings;
use cf_utilities::task_scope::Scope;
use chainflip_api::primitives::EpochIndex;
use chainflip_engine::{
	dot::retry_rpc::DotRetryRpcClient,
	settings::NodeContainer,
	state_chain_observer::client::{
		storage_api::StorageApi,
		stream_api::{StreamApi, UNFINALIZED},
		StateChainClient, STATE_CHAIN_CONNECTION,
	},
	witness::{
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
		dot::{filter_map_events, process_egress, proxy_added_witnessing, DotUnfinalisedSource},
	},
};
use futures::Future;

pub(super) async fn start<ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	witness_call: ProcessCall,
	settings: DepositTrackerSettings,
	env_params: EnvironmentParameters,
	state_chain_client: Arc<StateChainClient<()>>,
	state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient<()>, (), ()>,
) -> anyhow::Result<()>
where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let dot_client = DotRetryRpcClient::new(
		scope,
		NodeContainer { primary: settings.dot, backup: None },
		env_params.dot_genesis_hash,
	)?;

	let epoch_source = epoch_source
		.filter_map(
			|state_chain_client, _epoch_index, hash, _info| async move {
				state_chain_client
					.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
						hash,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
			},
			|_state_chain_client, _epoch, _block_hash, historic_info| async move { Some(historic_info) },
		)
		.await;

	let vaults = epoch_source.vaults::<cf_chains::Polkadot>().await;

	DotUnfinalisedSource::new(dot_client.clone())
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.strictly_monotonic()
		.chunk_by_vault(vaults.clone(), scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.dot_deposits(witness_call.clone())
		// Proxy added witnessing
		.then(proxy_added_witnessing)
		// Broadcast success
		.egress_items(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then(move |epoch, header| {
			process_egress(epoch, header, witness_call.clone(), dot_client.clone())
		})
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}
