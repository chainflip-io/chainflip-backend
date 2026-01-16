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

use std::{sync::Arc, time::Duration};

use cf_utilities::task_scope::Scope;
use chainflip_api::primitives::EpochIndex;
use chainflip_engine::{
	btc::{retry_rpc::BtcRetryRpcClient, rpc::BtcRpcApi},
	settings::NodeContainer,
	witness::{
		btc::{process_egress, source::BtcSource},
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
	},
};
use engine_sc_client::{
	stream_api::{StreamApi, UNFINALIZED},
	StateChainClient,
};

use futures::Future;
use tokio::time::sleep;

use crate::DepositTrackerSettings;

use super::EnvironmentParameters;
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
	let btc_client = BtcRetryRpcClient::new(
		scope,
		NodeContainer { primary: settings.btc, backup: None },
		env_params.chainflip_network.into(),
	)
	.await?;

	let vaults = epoch_source.vaults::<cf_chains::Bitcoin>().await;

	BtcSource::new(btc_client.clone())
		.strictly_monotonic()
		.then({
			let btc_client = btc_client.clone();
			move |header| {
				let btc_client = btc_client.clone();
				async move {
					loop {
						match btc_client.block(header.hash).await {
							Ok(block) => break (header.data, block.txdata),
							Err(err) => tracing::warn!(
								"Received error {err} when trying to query btc rpc. Retrying."
							),
						}
						sleep(Duration::from_secs(6)).await;
					}
				}
			}
		})
		.chunk_by_vault(vaults, scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.private_deposit_channels(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.btc_deposits(witness_call.clone())
		.egress_items(scope, state_chain_stream, state_chain_client)
		.await
		.then(move |epoch, header| process_egress(epoch, header, witness_call.clone()))
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}
