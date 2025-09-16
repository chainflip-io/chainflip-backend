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

use cf_utilities::task_scope::{self, Scope};
use futures::FutureExt;
use pallet_cf_elections::{
	electoral_system::ElectoralSystemTypes,
	electoral_systems::oracle_price::{
		chainlink::{ChainlinkAssetpair, ChainlinkPrice},
		primitives::UnixTime,
		state_machine::{should_vote_for_asset, ExternalChainStateVote, ExternalPriceChain},
	},
	VoteOf,
};
use sp_core::U256;
use state_chain_runtime::chainflip::generic_elections::*;

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		retry_rpc::{address_checker::AddressCheckerRetryRpcApi, EvmRetryRpcClient},
		rpc::{address_checker::PriceFeedData as EthPriceFeedData, EvmRpcSigningClient},
	},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};
use anyhow::{anyhow, Result};
use sol_prim::program_instructions::PriceFeedData as SolPriceFeedData;

/// IMPORTANT: These strings have to match with the price feed "description" as returned by
/// chainlink.
pub fn asset_pair_from_description(description: String) -> Option<ChainlinkAssetpair> {
	use ChainlinkAssetpair::*;
	match description.as_str() {
		"BTC / USD" => Some(BtcUsd),
		"ETH / USD" => Some(EthUsd),
		"SOL / USD" => Some(SolUsd),
		"USDT / USD" => Some(UsdtUsd),
		"USDC / USD" => Some(UsdcUsd),
		_ => None,
	}
}

#[derive(Clone)]
struct OraclePriceVoter {
	arb_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	eth_client: EvmRetryRpcClient<EvmRpcSigningClient>,
}

#[derive(Debug, Clone)]
struct PriceData {
	pub description: String,
	pub answer: i128,
	pub decimals: u8,
	pub timestamp: UnixTime,
}

impl From<SolPriceFeedData> for PriceData {
	fn from(value: SolPriceFeedData) -> Self {
		let SolPriceFeedData { round_id: _, slot: _, timestamp, answer, decimals, description } =
			value;
		Self { description, answer, decimals, timestamp: UnixTime { seconds: timestamp as u64 } }
	}
}

impl TryFrom<EthPriceFeedData> for PriceData {
	type Error = anyhow::Error;
	fn try_from(value: EthPriceFeedData) -> anyhow::Result<Self> {
		let EthPriceFeedData {
			round_id: _,
			answer,
			started_at: _,
			updated_at,
			answered_in_round: _,
			decimals,
			description,
		} = value;
		Ok(Self {
			description,
			answer: answer.try_into().map_err(anyhow::Error::new)?,
			decimals,
			timestamp: UnixTime {
				seconds: updated_at.try_into().map_err(|_| {
					anyhow!(
						"Unexpected timestamp when querying chainlink on ethereum {updated_at:?}"
					)
				})?,
			},
		})
	}
}

#[async_trait::async_trait]
impl VoterApi<ChainlinkOraclePriceES> for OraclePriceVoter {
	async fn vote(
		&self,
		settings: <ChainlinkOraclePriceES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ChainlinkOraclePriceES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<ChainlinkOraclePriceES>>, anyhow::Error> {
		let price_feeds = match properties.chain {
			ExternalPriceChain::Arbitrum => {
				let (_, _, price_feeds) = self
					.arb_client
					.query_price_feeds(
						settings.eth_address_checker,
						settings.eth_oracle_feeds.clone(),
					)
					.await?;
				price_feeds
					.into_iter()
					.filter_map(|data| {
						data.try_into().inspect_err(|err| tracing::warn!("{err}")).ok()
					})
					.collect::<Vec<PriceData>>()
			},
			ExternalPriceChain::Ethereum => {
				let (_, _, price_feeds) = self
					.eth_client
					.query_price_feeds(
						settings.eth_address_checker,
						settings.eth_oracle_feeds.clone(),
					)
					.await?;
				price_feeds
					.into_iter()
					.filter_map(|data| {
						data.try_into().inspect_err(|err| tracing::warn!("{err}")).ok()
					})
					.collect::<Vec<PriceData>>()
			},
		};

		tracing::debug!(
			"Price feed results when querying {:?}: {:?}",
			properties.chain,
			price_feeds
		);

		let prices: BTreeMap<_, _> = price_feeds
			.into_iter()
			.filter_map(|price_data| {
				let Some(asset_pair) = asset_pair_from_description(price_data.description.clone())
				else {
					tracing::debug!(
						"Ignoring price data with unknown description: {:?}",
						price_data.description
					);
					return None;
				};

				let positive_price: U256 = if price_data.answer < 0 {
					tracing::debug!(
						"Ignoring negative price data for {:?}: {}",
						asset_pair,
						price_data.answer
					);
					return None;
				} else {
					(price_data.answer as u128).into()
				};

				if ChainlinkPrice::denominator() != 10u32.pow(price_data.decimals as u32).into() {
					tracing::debug!(
						"Ignoring price for {:?} with wrong number of decimals ({})",
						asset_pair,
						price_data.decimals
					);
					return None;
				}

				let price = ChainlinkPrice::from_raw(positive_price);
				Some((asset_pair, (price_data.timestamp, price)))
			})
			.collect();

		let should_vote = prices.iter().any(|(asset, result)| {
			if let Some(conditions) = properties.assets.get(asset) {
				should_vote_for_asset(result, conditions)
			} else {
				// if the asset isn't mentioned in the conditions we ignore it when checking
				// whether we should submit a vote
				false
			}
		});

		Ok(if should_vote {
			tracing::debug!("Submitting oracle result for election properties: {properties:?}, result: {prices:?}");
			Some(ExternalChainStateVote { price: prices })
		} else {
			tracing::debug!("Skipping oracle result for election properties: {properties:?}, result: {prices:?}");
			None
		})
	}
}

use std::{collections::BTreeMap, sync::Arc};
pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	arb_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	eth_client: EvmRetryRpcClient<EvmRpcSigningClient>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient:
		StorageApi + ChainApi + SignedExtrinsicApi + ElectoralApi<()> + 'static + Send + Sync,
{
	tracing::info!("Starting generic election witnessing");
	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<GenericElectoralSystemRunner, _>::new((OraclePriceVoter {
						arb_client: arb_client.clone(),
						eth_client: eth_client.clone(),
					},)),
					None,
					"GenericElections",
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
