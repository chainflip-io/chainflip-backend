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
		primitives::UnixTime,
		state_machine::{should_vote_for_asset, ExternalChainStateVote, ExternalPriceChain},
	},
	VoteOf,
};
use state_chain_runtime::chainflip::generic_elections::*;

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		retry_rpc::{address_checker::AddressCheckerRetryRpcApi, EvmRetryRpcClient},
		rpc::{address_checker::PriceFeedData as EthPriceFeedData, EvmRpcSigningClient},
	},
	sol::retry_rpc::SolRetryRpcClient,
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::sol::oracle_witnessing::get_price_feeds,
};
use anyhow::Result;
use pallet_cf_elections::electoral_systems::oracle_price::price::ChainlinkAssetPair;
use sol_prim::program_instructions::PriceFeedData as SolPriceFeedData;
use sp_core::U256;

/// IMPORTANT: These strings have to match with the price feed "description" as returned by
/// chainlink.
pub fn asset_pair_from_description(description: String) -> Option<ChainlinkAssetPair> {
	use ChainlinkAssetPair::*;
	match description.as_str() {
		"BTC / USD" => Some(BtcUsd),
		"ETH / USD" => Some(EthUsd),
		"SOL / USD" => Some(EthUsd),
		"USDT / USD" => Some(EthUsd),
		"USDC / USD" => Some(EthUsd),
		_ => None,
	}
}

#[derive(Clone)]
struct OraclePriceVoter {
	sol_client: SolRetryRpcClient,
	eth_client: EvmRetryRpcClient<EvmRpcSigningClient>,
}

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

impl From<EthPriceFeedData> for PriceData {
	fn from(value: EthPriceFeedData) -> Self {
		let EthPriceFeedData {
			round_id: _,
			answer,
			started_at: _,
			updated_at,
			answered_in_round: _,
			decimals,
			description,
		} = value;
		Self {
			description,
			answer: answer.try_into().unwrap(),
			decimals,
			timestamp: UnixTime { seconds: updated_at.try_into().unwrap() },
		}
	}
}

#[async_trait::async_trait]
impl VoterApi<OraclePriceES> for OraclePriceVoter {
	async fn vote(
		&self,
		settings: <OraclePriceES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <OraclePriceES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<OraclePriceES>>, anyhow::Error> {
		let price_feeds = match properties.chain {
			ExternalPriceChain::Solana => {
				let (price_feeds, _, _) = get_price_feeds(
					&self.sol_client,
					settings.sol_oracle_query_helper.clone(),
					settings.sol_oracle_program_id.clone(),
					settings.sol_oracle_feeds.clone(),
					None,
				)
				.await?;
				price_feeds.into_iter().map(Into::into).collect::<Vec<PriceData>>()
			},
			ExternalPriceChain::Ethereum => {
				let (_, _, price_feeds) = self
					.eth_client
					.query_price_feeds(
						settings.eth_contract_address.clone(),
						settings.eth_oracle_feeds.clone(),
					)
					.await?;
				price_feeds.into_iter().map(Into::into).collect::<Vec<PriceData>>()
			},
		};

		let prices: BTreeMap<_, _> = price_feeds
			.into_iter()
			.filter_map(|price_data| {
				let Some(asset_pair) = asset_pair_from_description(price_data.description.clone())
				else {
					tracing::debug!(
						"Got price data with unknown description: {:?}",
						price_data.description
					);
					return None;
				};

				let Ok(positive_price) = price_data.answer.try_into() else {
					tracing::debug!("Got negative price data: {}", price_data.answer);
					return None;
				};

				if ChainlinkPrice::denominator() != 10u32.pow(price_data.decimals as u32).into() {
					tracing::debug!("Got wrong number of decimals: {}", price_data.decimals);
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
	sol_client: SolRetryRpcClient,
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
						sol_client: sol_client.clone(),
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
