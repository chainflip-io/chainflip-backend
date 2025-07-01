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

use crate::{
	btc::cached_rpc::BtcCachingClient,
	db::PersistentKeyDB,
	dot::retry_rpc::DotRetryRpcClient,
	evm::{retry_rpc::EvmRetryRpcClient, rpc::EvmRpcSigningClient},
	sol::retry_rpc::SolRetryRpcClient,
	state_chain_observer::client::{
		chain_api::ChainApi,
		electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED, UNFINALIZED},
	},
};
use cf_utilities::task_scope::Scope;
use futures::try_join;
use state_chain_runtime::{BitcoinInstance, SolanaInstance};

use super::common::epoch_source::EpochSource;

use anyhow::Result;

/// Starts all the witnessing tasks.
// It's important that this function is not blocking, at any point, even if there is no connection
// to any or all chains. This implies that the `start` function for each chain should not be
// blocking. The chains must be able to witness independently, and if this blocks at any
// point it means that on start up this will block, and the state chain observer will not start.
pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	eth_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	arb_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	btc_client: BtcCachingClient,
	dot_client: DotRetryRpcClient,
	sol_client: SolRetryRpcClient,
	hub_client: DotRetryRpcClient,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: impl StreamApi<FINALIZED> + Clone,
	_unfinalised_state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<SolanaInstance>
		+ ElectoralApi<BitcoinInstance>
		+ 'static
		+ Send
		+ Sync,
{
	let epoch_source =
		EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
			.await
			.participating(state_chain_client.account_id())
			.await;

	let witness_call = {
		let state_chain_client = state_chain_client.clone();
		move |call, epoch_index| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let _ = state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(call),
						epoch_index,
					})
					.await;
			}
		}
	};

	let _prewitness_call = {
		let state_chain_client = state_chain_client.clone();
		move |call, epoch_index| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let _ = state_chain_client
					.finalize_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(
							pallet_cf_witnesser::Call::prewitness_and_execute {
								call: Box::new(call),
							}
							.into(),
						),
						epoch_index,
					})
					.await;
			}
		}
	};

	let start_eth = super::eth::start(
		scope,
		eth_client,
		witness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_dot = super::dot::start(
		scope,
		dot_client,
		witness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_arb = super::arb::start(
		scope,
		arb_client,
		witness_call.clone(),
		state_chain_client.clone(),
		state_chain_stream.clone(),
		epoch_source.clone(),
		db.clone(),
	);

	let start_sol = super::sol::start(scope, sol_client, state_chain_client.clone());

	let start_btc = super::btc::start(scope, btc_client, state_chain_client.clone());

	let start_hub = super::hub::start(
		scope,
		hub_client,
		witness_call.clone(),
		state_chain_client,
		state_chain_stream,
		epoch_source,
		db,
	);

	try_join!(start_eth, start_dot, start_arb, start_sol, start_btc, start_hub)?;

	Ok(())
}
