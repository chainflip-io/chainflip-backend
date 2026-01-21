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

use cf_chains::{Chain, Ethereum};
use cf_utilities::task_scope;
use chainflip_api::primitives::{
	chains::assets::eth::Asset as EthAsset, Asset, EpochIndex, ForeignChain,
};
use std::sync::Arc;

use chainflip_engine::{
	evm::{retry_rpc::EvmRetryRpcClient, rpc::EvmRpcClient},
	settings::NodeContainer,
	witness::{
		common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
		eth::EthCallBuilder,
		evm::{
			erc20_deposits::{
				flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents, wbtc::WbtcEvents,
			},
			source::EvmSource,
		},
	},
};
use engine_sc_client::{
	stream_api::{StreamApi, UNFINALIZED},
	StateChainClient,
};

use crate::DepositTrackerSettings;

use super::EnvironmentParameters;

pub(super) async fn start<ProcessCall, ProcessingFut>(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	state_chain_client: Arc<StateChainClient<()>>,
	state_chain_stream: impl StreamApi<UNFINALIZED> + Clone,
	settings: DepositTrackerSettings,
	env_params: EnvironmentParameters,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient<()>, (), ()>,
	witness_call: ProcessCall,
) -> anyhow::Result<()>
where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: futures::Future<Output = ()> + Send + 'static,
{
	let eth_client = {
		let nodes = NodeContainer { primary: settings.eth.clone(), backup: None };

		EvmRetryRpcClient::<EvmRpcClient>::new(
			scope,
			nodes,
			env_params.eth_chain_id.into(),
			"eth_rpc",
			"eth_subscribe_client",
			"Ethereum",
			Ethereum::WITNESS_PERIOD,
		)?
	};

	let vaults = epoch_source.vaults::<Ethereum>().await;
	let eth_source = EvmSource::<_, Ethereum>::new(eth_client.clone())
		.strictly_monotonic()
		.chunk_by_vault(vaults, scope);

	let eth_source_deposit_addresses = eth_source
		.clone()
		.deposit_addresses(scope, state_chain_stream, state_chain_client.clone())
		.await;

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdcEvents>(
			witness_call.clone(),
			eth_client.clone(),
			EthAsset::Usdc,
			env_params.eth_usdc_contract_address,
		)
		.await?
		.logging("witnessing USDCDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, FlipEvents>(
			witness_call.clone(),
			eth_client.clone(),
			EthAsset::Flip,
			env_params.eth_flip_contract_address,
		)
		.await?
		.logging("witnessing FlipDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, UsdtEvents>(
			witness_call.clone(),
			eth_client.clone(),
			EthAsset::Usdt,
			env_params.eth_usdt_contract_address,
		)
		.await?
		.logging("witnessing USDTDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.erc20_deposits::<_, _, _, WbtcEvents>(
			witness_call.clone(),
			eth_client.clone(),
			EthAsset::Wbtc,
			env_params.eth_wbtc_contract_address,
		)
		.await?
		.logging("witnessing WBTCDeposits")
		.spawn(scope);

	eth_source_deposit_addresses
		.clone()
		.ethereum_deposits(
			witness_call.clone(),
			eth_client.clone(),
			EthAsset::Eth,
			env_params.eth_address_checker_address,
			env_params.eth_vault_address,
		)
		.await
		.logging("witnessing EthereumDeposits")
		.spawn(scope);

	eth_source
		.clone()
		.vault_witnessing::<EthCallBuilder, _, _, _>(
			witness_call.clone(),
			eth_client.clone(),
			env_params.eth_vault_address,
			Asset::Eth,
			ForeignChain::Ethereum,
			env_params.eth_supported_erc20_tokens.clone(),
		)
		.logging("witnessing Vault")
		.spawn(scope);

	eth_source
		.clone()
		.key_manager_witnessing(
			witness_call.clone(),
			eth_client.clone(),
			env_params.eth_key_manager_address,
		)
		.logging("witnessing KeyManager")
		.spawn(scope);

	Ok(())
}
