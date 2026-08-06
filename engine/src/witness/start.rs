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
	dot::cached_rpc::DotCachingClient,
	evm::{cached_rpc::EvmCachingClient, rpc::EvmRpcSigningClient},
	sol::retry_rpc::SolRetryRpcClient,
	tron::{
		cached_rpc::TronCachingClient,
		rpc::{TronRpcClient, TronRpcSigningClient},
	},
};
use cf_utilities::task_scope::Scope;
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi,
};
use futures::try_join;
use state_chain_runtime::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, BscInstance, EthereumInstance,
	SolanaInstance, TronInstance,
};

use anyhow::Result;

/// Starts all the witnessing tasks.
// It's important that this function is not blocking, at any point, even if there is no connection
// to any or all chains. This implies that the `start` function for each chain should not be
// blocking. The chains must be able to witness independently, and if this blocks at any
// point it means that on start up this will block, and the state chain observer will not start.
pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	eth_client: EvmCachingClient<EvmRpcSigningClient>,
	arb_client: EvmCachingClient<EvmRpcSigningClient>,
	btc_client: BtcCachingClient,
	sol_client: SolRetryRpcClient,
	hub_client: DotCachingClient,
	tron_client: TronCachingClient<TronRpcSigningClient<TronRpcClient>>,
	bsc_client: EvmCachingClient<EvmRpcSigningClient>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<SolanaInstance>
		+ ElectoralApi<BitcoinInstance>
		+ ElectoralApi<()>
		+ ElectoralApi<EthereumInstance>
		+ ElectoralApi<ArbitrumInstance>
		+ ElectoralApi<AssethubInstance>
		+ ElectoralApi<TronInstance>
		+ ElectoralApi<BscInstance>
		+ 'static
		+ Send
		+ Sync,
{
	let start_arb =
		super::arb_elections::start(scope, arb_client.clone(), state_chain_client.clone());

	let start_bsc =
		super::bsc_elections::start(scope, bsc_client.clone(), state_chain_client.clone());

	let start_sol = super::sol::start(scope, sol_client, state_chain_client.clone());

	let start_btc = super::btc::start(scope, btc_client, state_chain_client.clone());

	let start_eth =
		super::eth_elections::start(scope, eth_client.clone(), state_chain_client.clone());

	let start_hub = super::hub_elections::start(scope, hub_client, state_chain_client.clone());

	let start_tron = super::tron_elections::start(scope, tron_client, state_chain_client.clone());

	let start_generic_elections =
		super::generic_elections::start(scope, arb_client, eth_client, state_chain_client);

	try_join!(
		start_eth,
		start_arb,
		start_bsc,
		start_sol,
		start_btc,
		start_tron,
		start_hub,
		start_generic_elections
	)?;

	Ok(())
}
