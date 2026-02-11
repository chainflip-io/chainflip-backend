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
	evm::retry_rpc::node_interface::NodeInterfaceRetryRpcApiWithResult,
	witness::{
		common::block_height_witnesser::{witness_headers, HeaderClient},
		evm::{
			contract_common::{events_at_block, query_election_block},
			erc20_deposits::Erc20Events,
			key_manager::{handle_key_manager_events, KeyManagerEventConfig, KeyManagerEvents},
			vault::{handle_vault_events, VaultEvents},
		},
	},
};
use cf_chains::{
	arb::ArbitrumTrackedData,
	assets,
	witness_period::{block_witness_range, block_witness_root, BlockWitnessRange, SaturatingStep},
	Arbitrum, ChainWitnessConfig,
};
use cf_primitives::chains::assets::arb::Asset as ArbAsset;
use cf_utilities::task_scope::{self, Scope};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use ethers::types::{Bloom, Bytes};
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::primitives::Header,
		block_witnesser::state_machine::BWElectionProperties,
	},
	ElectoralSystemTypes, VoteOf,
};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::witnessing::arbitrum_elections::{
		ArbitrumBlockHeightWitnesserES, ArbitrumChain, ArbitrumDepositChannelWitnessingES,
		ArbitrumElectoralSystemRunner, ArbitrumFeeTracking, ArbitrumKeyManagerWitnessingES,
		ArbitrumLiveness, ArbitrumVaultDepositWitnessingES, ARBITRUM_MAINNET_SAFETY_BUFFER,
	},
	ArbitrumInstance,
};
use std::{collections::HashMap, sync::Arc};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		rpc::EvmRpcSigningClient,
	},
};

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct ArbitrumBlockHeightWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}

#[async_trait::async_trait]
impl HeaderClient<ArbitrumChain> for ArbitrumBlockHeightWitnesserVoter {
	async fn best_block_header(&self) -> anyhow::Result<Header<ArbitrumChain>> {
		self.block_header_by_height(self.best_block_number().await?).await
	}

	async fn block_header_by_height(
		&self,
		height: BlockWitnessRange<Arbitrum>,
	) -> anyhow::Result<Header<ArbitrumChain>> {
		let range = height.into_range_inclusive();
		let (block_start, block_end) = futures::try_join!(
			self.client.block((*range.start()).into()),
			self.client.block((*range.end()).into())
		)?;
		Ok(Header {
			block_height: height,
			hash: block_end.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block_start.parent_hash,
		})
	}
	async fn best_block_number(&self) -> anyhow::Result<BlockWitnessRange<Arbitrum>> {
		let best_block = self.client.get_block_number().await?.low_u64();
		let range =
			block_witness_range(<Arbitrum as ChainWitnessConfig>::WITNESS_PERIOD, best_block);
		let block_witness_range = BlockWitnessRange::try_new(block_witness_root(
			<Arbitrum as ChainWitnessConfig>::WITNESS_PERIOD,
			best_block,
		))
		.map_err(|_| anyhow::anyhow!("Failed to build BlockWitnessRange"))?;
		if best_block == *range.end() {
			return Ok(block_witness_range);
		}
		Ok(block_witness_range.saturating_backward(1))
	}
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumBlockHeightWitnesserES> for ArbitrumBlockHeightWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<ArbitrumBlockHeightWitnesserES, _, ArbitrumChain>(
			self,
			properties,
			ARBITRUM_MAINNET_SAFETY_BUFFER,
			"ARB BHW",
		)
		.await
	}
}

#[derive(Clone)]
pub struct ArbitrumDepositChannelWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	address_checker_address: H160,
	vault_address: H160,
	usdc_contract_address: H160,
	usdt_contract_address: H160,
}

#[async_trait::async_trait]
impl crate::witness::evm::contract_common::DepositChannelWitnesserConfig<Arbitrum, ArbitrumChain>
	for ArbitrumDepositChannelWitnesserVoter
{
	fn client(&self) -> &EvmCachingClient<EvmRpcSigningClient> {
		&self.client
	}

	fn address_checker_address(&self) -> H160 {
		self.address_checker_address
	}

	fn vault_address(&self) -> H160 {
		self.vault_address
	}

	async fn get_events_for_erc20_asset(
		&self,
		asset: ArbAsset,
		bloom: Option<Bloom>,
		block_height: BlockWitnessRange<Arbitrum>,
		block_hash: sp_core::H256,
	) -> Result<Option<Vec<crate::witness::evm::contract_common::Event<Erc20Events>>>> {
		use crate::witness::evm::{
			contract_common::events_at_block,
			erc20_deposits::{usdc::UsdcEvents, usdt::UsdtEvents},
		};

		let events = match asset {
			ArbAsset::ArbUsdc =>
				events_at_block::<cf_chains::Arbitrum, UsdcEvents, ArbitrumChain, _>(
					bloom,
					block_height,
					block_hash,
					self.usdc_contract_address,
					&self.client,
				)
				.await?
				.into_iter()
				.map(|event| crate::witness::evm::contract_common::Event {
					event_parameters: event.event_parameters.into(),
					tx_hash: event.tx_hash,
					log_index: event.log_index,
				})
				.collect::<Vec<_>>(),
			ArbAsset::ArbUsdt =>
				events_at_block::<cf_chains::Arbitrum, UsdtEvents, ArbitrumChain, _>(
					bloom,
					block_height,
					block_hash,
					self.usdt_contract_address,
					&self.client,
				)
				.await?
				.into_iter()
				.map(|event| crate::witness::evm::contract_common::Event {
					event_parameters: event.event_parameters.into(),
					tx_hash: event.tx_hash,
					log_index: event.log_index,
				})
				.collect::<Vec<_>>(),
			_ => return Ok(None), // Skip unsupported assets
		};
		Ok(Some(events))
	}
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumDepositChannelWitnessingES> for ArbitrumDepositChannelWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumDepositChannelWitnessingES>>, anyhow::Error> {
		use state_chain_runtime::chainflip::witnessing::arbitrum_elections::ArbitrumChain;

		let BWElectionProperties {
			block_height, properties: deposit_addresses, election_type, ..
		} = properties;

		let (witnesses, return_block_hash) =
			crate::witness::evm::contract_common::witness_deposit_channels_generic::<
				cf_chains::Arbitrum,
				ArbitrumChain,
				_,
			>(self, block_height, election_type, deposit_addresses)
			.await?;

		Ok(Some((witnesses, return_block_hash)))
	}
}

#[derive(Clone)]
pub struct ArbitrumVaultDepositWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	vault_address: H160,
	supported_assets: HashMap<H160, assets::arb::Asset>,
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumVaultDepositWitnessingES> for ArbitrumVaultDepositWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumVaultDepositWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _vault, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Arbitrum>(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Arbitrum, VaultEvents, ArbitrumChain, _>(
			block.bloom,
			block_height,
			block.hash,
			self.vault_address,
			&self.client,
		)
		.await?;

		let result = handle_vault_events(&self.supported_assets, events, *block_height.root())?;

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct ArbitrumKeyManagerWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	key_manager_address: H160,
}

impl KeyManagerEventConfig for ArbitrumKeyManagerWitnesserVoter {
	type Chain = Arbitrum;
	type Instance = ArbitrumInstance;

	fn client(&self) -> &EvmCachingClient<EvmRpcSigningClient> {
		&self.client
	}
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumKeyManagerWitnessingES> for ArbitrumKeyManagerWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumKeyManagerWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _key_manager, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Arbitrum>(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Arbitrum, KeyManagerEvents, ArbitrumChain, _>(
			block.bloom,
			block_height,
			block.hash,
			self.key_manager_address,
			&self.client,
		)
		.await?;

		let result = handle_key_manager_events(self, events, *block_height.root()).await?;

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct ArbitrumFeeVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumFeeTracking> for ArbitrumFeeVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <ArbitrumFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumFeeTracking>>, anyhow::Error> {
		let (_, _, l2_base_fee, l1_base_fee_estimate) = self
			.client
			.gas_estimate_components(
				// Using zero address as a proxy destination address for the gas estimation.
				H160::default(),
				false,
				// Using empty data for the gas estimation
				Bytes::default(),
			)
			.await?;

		Ok(Some(ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().expect("Base fee should fit u128"),
			l1_base_fee_estimate: l1_base_fee_estimate
				.try_into()
				.expect("L1 base fee should fit u128"),
		}))
	}
}

#[derive(Clone)]
pub struct ArbitrumLivenessVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumLiveness> for ArbitrumLivenessVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumLiveness>>, anyhow::Error> {
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
		+ ElectoralApi<ArbitrumInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting ARB witness");

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_erc20_tokens: HashMap<ArbAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::ArbitrumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch Arbitrum supported assets")?;

	let usdc_contract_address =
		*supported_erc20_tokens.get(&ArbAsset::ArbUsdc).context("USDC not supported")?;

	let usdt_contract_address =
		*supported_erc20_tokens.get(&ArbAsset::ArbUsdt).context("USDT not supported")?;

	let supported_erc20_tokens: HashMap<H160, assets::arb::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset))
		.collect();
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<ArbitrumElectoralSystemRunner, _>::new((
						ArbitrumBlockHeightWitnesserVoter { client: client.clone() },
						ArbitrumDepositChannelWitnesserVoter {
							client: client.clone(),
							address_checker_address,
							vault_address,
							usdc_contract_address,
							usdt_contract_address,
						},
						ArbitrumVaultDepositWitnesserVoter {
							client: client.clone(),
							vault_address,
							supported_assets: supported_erc20_tokens.clone(),
						},
						ArbitrumKeyManagerWitnesserVoter {
							client: client.clone(),
							key_manager_address,
						},
						ArbitrumFeeVoter { client: client.clone() },
						ArbitrumLivenessVoter { client: client.clone() },
					)),
					Some(client.cache_invalidation_senders),
					"Arbitrum",
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
