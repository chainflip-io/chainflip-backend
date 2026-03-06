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
	evm::{event::Event, retry_rpc::node_interface::NodeInterfaceRetryRpcApiWithResult},
	witness::{
		common::{
			block_height_witnesser::witness_headers, block_witnesser::GenericBwVoter,
			traits::WitnessClient,
		},
		evm::{
			erc20_deposits::{usdc::UsdcEvents, usdt::UsdtEvents},
			key_manager::KeyManagerEvents,
			vault::VaultEvents,
			EvmDepositChannelWitnessingConfig, EvmKeyManagerWitnessingConfig, EvmVoter,
			VaultDepositWitnessingConfig,
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
use ethers::types::Bytes;
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::{
		primitives::Header, ChainBlockHashOf, ChainBlockNumberOf, ChainTypes,
	},
	ElectoralSystemTypes, VoteOf,
};
use sp_core::{H160, H256};
use state_chain_runtime::{
	chainflip::witnessing::arbitrum_elections::{
		ArbitrumBlockHeightWitnesserES, ArbitrumChain, ArbitrumElectoralSystemRunner,
		ArbitrumFeeTracking, ArbitrumLiveness, ARBITRUM_MAINNET_SAFETY_BUFFER,
	},
	ArbitrumInstance,
};
use std::{collections::HashMap, sync::Arc};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		event::EvmEventSource,
		rpc::{address_checker::AddressState, EvmRpcSigningClient},
	},
	witness::evm::{
		contract_common::address_states, EvmAddressStateClient, EvmBlockQuery, EvmEventClient,
	},
};

use anyhow::{Context, Result};

#[derive_where::derive_where(Clone, Debug;)]
pub struct EvmBlockRangeQuery<C: ChainWitnessConfig> {
	pub blocks_heights: BlockWitnessRange<C>,
	pub parent_hash_of_first_block: H256,
	pub hash_of_last_block: H256,
}

impl<W: ChainWitnessConfig> EvmBlockQuery for EvmBlockRangeQuery<W> {
	fn get_lowest_block_height_of_query(&self) -> u64 {
		*self.blocks_heights.into_range_inclusive().start()
	}
}

trait RangeWitnessConfig = ChainWitnessConfig<ChainBlockNumber = u64> + Sync + Send;

trait EvmBlockRangeChainType<C: ChainWitnessConfig> =
	ChainTypes<ChainBlockNumber = BlockWitnessRange<C>, ChainBlockHash = H256> + Sync + Send;

#[async_trait::async_trait]
impl<Chain: EvmBlockRangeChainType<W>, W: RangeWitnessConfig> WitnessClient<Chain>
	for EvmVoter<Chain, EvmBlockRangeQuery<W>>
{
	type BlockQuery = EvmBlockRangeQuery<W>;

	// --- BHW methods ---

	async fn best_block_header(&self) -> Result<Header<Chain>> {
		self.block_header_by_height(self.best_block_number().await?).await
	}

	async fn block_header_by_height(&self, height: BlockWitnessRange<W>) -> Result<Header<Chain>> {
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

	async fn best_block_number(&self) -> Result<BlockWitnessRange<W>> {
		let best_block = self.client.get_block_number().await?.low_u64();
		let range = block_witness_range(W::WITNESS_PERIOD, best_block);
		let block_witness_range =
			BlockWitnessRange::try_new(block_witness_root(W::WITNESS_PERIOD, best_block))
				.map_err(|_| anyhow::anyhow!("Failed to build BlockWitnessRange"))?;
		if best_block == *range.end() {
			return Ok(block_witness_range);
		}
		Ok(block_witness_range.saturating_backward(1))
	}

	// --- BW methods ---

	async fn block_query_from_hash_and_height(
		&self,
		hash: ChainBlockHashOf<Chain>,
		height: ChainBlockNumberOf<Chain>,
	) -> Result<Self::BlockQuery> {
		let header = self.block_header_by_height(height).await?;
		if header.hash != hash {
			return Err(anyhow::anyhow!(
				"Block hash from RPC ({}) doesn't match election block hash: {}",
				header.hash,
				hash
			));
		}
		Ok(EvmBlockRangeQuery {
			blocks_heights: height,
			hash_of_last_block: header.hash,
			parent_hash_of_first_block: header.parent_hash,
		})
	}

	async fn block_query_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<Self::BlockQuery> {
		let header = self.block_header_by_height(height).await?;
		Ok(EvmBlockRangeQuery {
			blocks_heights: height,
			hash_of_last_block: header.hash,
			parent_hash_of_first_block: header.parent_hash,
		})
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<(Self::BlockQuery, Chain::ChainBlockHash)> {
		let header = self.block_header_by_height(height).await?;
		Ok((
			EvmBlockRangeQuery {
				blocks_heights: height,
				hash_of_last_block: header.hash,
				parent_hash_of_first_block: header.parent_hash,
			},
			header.hash,
		))
	}
}

#[async_trait::async_trait]
impl<Chain: EvmBlockRangeChainType<W>, W: RangeWitnessConfig> EvmEventClient<Chain>
	for EvmVoter<Chain, EvmBlockRangeQuery<W>>
{
	async fn events_from_block_query<Data: std::fmt::Debug>(
		&self,
		EvmEventSource { contract_address, event_type }: &EvmEventSource<Data>,
		query: Self::BlockQuery,
	) -> Result<Vec<Event<Data>>> {
		Ok(self
			.client
			.get_logs_range(query.blocks_heights.into_range_inclusive(), *contract_address)
			.await?
			.into_iter()
			.filter_map(|unparsed_log| -> Option<Event<_>> {
				event_type
					.parse_log(unparsed_log)
					.map_err(|err| {
						tracing::error!(
						    "event for contract {} could not be decoded in block range {:?}. Error: {err}",
						    contract_address, query.blocks_heights
					    )
					})
					.ok()
			})
			.collect())
	}
}

#[async_trait::async_trait]
impl<Chain: EvmBlockRangeChainType<W>, W: RangeWitnessConfig> EvmAddressStateClient<Chain>
	for EvmVoter<Chain, EvmBlockRangeQuery<W>>
{
	async fn address_states(
		&self,
		address_checker_address: H160,
		query: Self::BlockQuery,
		addresses: Vec<H160>,
	) -> Result<HashMap<H160, (AddressState, AddressState)>> {
		address_states(
			&self.client,
			address_checker_address,
			query.parent_hash_of_first_block,
			query.hash_of_last_block,
			addresses,
		)
		.await
	}
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumBlockHeightWitnesserES>
	for EvmVoter<ArbitrumChain, EvmBlockRangeQuery<Arbitrum>>
{
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

	let supported_asset_address_and_event_type = [
		(ArbAsset::ArbUsdc, EvmEventSource::new::<UsdcEvents>(usdc_contract_address)),
		(ArbAsset::ArbUsdt, EvmEventSource::new::<UsdtEvents>(usdt_contract_address)),
	]
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
					CompositeVoter::<ArbitrumElectoralSystemRunner, _>::new((
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
