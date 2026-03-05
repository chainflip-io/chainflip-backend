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

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		event::EvmEventSource,
		rpc::EvmRpcSigningClient,
	},
	witness::{
		arb_elections::EvmBlockRangeQuery,
		common::{block_height_witnesser::witness_headers, block_witnesser::GenericBwVoter},
		evm::{
			erc20_deposits::usdt::UsdtEvents, key_manager::KeyManagerEvents, vault::VaultEvents,
			EvmDepositChannelWitnessingConfig, EvmKeyManagerWitnessingConfig, EvmVoter,
			VaultDepositWitnessingConfig,
		},
	},
};
use cf_chains::{assets, bsc::BscTrackedData, Bsc};
use cf_primitives::chains::assets::bsc::Asset as BscAsset;
use cf_utilities::{
	context,
	task_scope::{self, Scope},
};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use futures::FutureExt;
use pallet_cf_elections::{ElectoralSystemTypes, VoteOf};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::witnessing::bsc_elections::{
		BscBlockHeightWitnesserES, BscChain, BscElectoralSystemRunner, BscFeeTracking, BscLiveness,
		BSC_MAINNET_SAFETY_BUFFER,
	},
	BscInstance,
};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};

// --- block height witnessing ---

#[async_trait::async_trait]
impl VoterApi<BscBlockHeightWitnesserES> for EvmVoter<BscChain, EvmBlockRangeQuery<Bsc>> {
	async fn vote(
		&self,
		_settings: <BscBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BscBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BscBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<BscBlockHeightWitnesserES, _, BscChain>(
			self,
			properties,
			BSC_MAINNET_SAFETY_BUFFER,
			"BSC BHW",
		)
		.await
	}
}

// --- fee witnessing ---

#[derive(Clone)]
pub struct BscFeeVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}

#[async_trait::async_trait]
impl VoterApi<BscFeeTracking> for BscFeeVoter {
	async fn vote(
		&self,
		_settings: <BscFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <BscFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BscFeeTracking>>, anyhow::Error> {
		let best_block_number = self.client.get_block_number().await?;
		let fee_history = self
			.client
			.fee_history(1u64.into(), best_block_number.low_u64().into(), vec![])
			.await?;

		Ok(Some(BscTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.last())?)
				.try_into()
				.expect("Base fee should fit u128"),
		}))
	}
}

// --- liveness witnessing ---

#[derive(Clone)]
pub struct BscLivenessVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}

#[async_trait::async_trait]
impl VoterApi<BscLiveness> for BscLivenessVoter {
	async fn vote(
		&self,
		_settings: <BscLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <BscLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<BscLiveness>>, anyhow::Error> {
		let block = self.client.block(properties.into()).await?;
		Ok(Some(block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?))
	}
}

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: EvmCachingClient<EvmRpcSigningClient>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<BscInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting BSC witness");

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::BscKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::BscVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::BscAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_erc20_tokens: HashMap<BscAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::BscSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch BSC supported assets")?;

	let usdt_contract_address =
		*supported_erc20_tokens.get(&BscAsset::BscUsdt).context("USDT not supported")?;

	let supported_erc20_tokens: HashMap<H160, assets::bsc::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset))
		.collect();

	let supported_asset_address_and_event_type =
		[(BscAsset::BscUsdt, EvmEventSource::new::<UsdtEvents>(usdt_contract_address))]
			.into_iter()
			.collect();

	let vault_event_source = EvmEventSource::new::<VaultEvents>(vault_address);
	let key_manager_event_source = EvmEventSource::new::<KeyManagerEvents>(key_manager_address);

	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<BscElectoralSystemRunner, _>::new((
						EvmVoter::new(client.clone()),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EvmDepositChannelWitnessingConfig {
								address_checker_address,
								vault_contract: vault_event_source.clone(),
								supported_assets: supported_asset_address_and_event_type,
							},
						),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							VaultDepositWitnessingConfig {
								vault: vault_event_source,
								supported_assets: supported_erc20_tokens.clone(),
							},
						),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EvmKeyManagerWitnessingConfig { key_manager: key_manager_event_source },
						),
						BscFeeVoter { client: client.clone() },
						BscLivenessVoter { client: client.clone() },
					)),
					Some(client.cache_invalidation_senders),
					"Bsc",
				)
				.continuously_vote()
				.await;

				Ok(())
			}
			.boxed()
		})
		.await
	});

	Ok(())
}
