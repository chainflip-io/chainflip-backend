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

//! Chainlink specific types

use crate::electoral_systems::{
	oracle_price::{price::*, state_machine::*},
	state_machine::common_imports::*,
};
use cf_amm_math::Price;
use sp_std::iter;

def_derive! {
	/// Representation of the asset pairs as returned in the `description` field of chainlink responses.
	#[derive(TypeInfo, Sequence, PartialOrd, Ord, Copy)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ChainlinkAssetpair {
		BtcUsd,
		EthUsd,
		SolUsd,
		UsdcUsd,
		UsdtUsd
	}
}

impl AssetPairTrait for ChainlinkAssetpair {
	fn to_price_unit(&self) -> PriceUnit {
		use ChainlinkAssetpair::*;
		use PriceAsset::*;
		let base_asset = match self {
			BtcUsd => Btc,
			EthUsd => Eth,
			SolUsd => Sol,
			UsdcUsd => Usdc,
			UsdtUsd => Usdt,
		};
		PriceUnit { base_asset, quote_asset: PriceAsset::Usd }
	}
}

/// Encoding of price mirroring the encoding we get from chainlink:
/// Integer with fractional part consisting of 8 decimals.
pub type ChainlinkPrice = Fraction<99_999_999>;

// Used by tests.
#[cfg(test)]
pub fn get_all_latest_prices_with_statechain_encoding<T>(
	state: &OraclePriceTracker<T>,
) -> BTreeMap<PriceAsset, (Price, PriceStatus)>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice>,
{
	all::<ChainlinkAssetpair>()
		.filter_map(|assetpair| {
			get_latest_price_with_statechain_encoding(state, assetpair)
				.map(|result| (assetpair.to_price_unit().base_asset, result))
		})
		.collect()
}

// Used for the price API exposed to other pallets.
pub fn get_latest_price_with_statechain_encoding<T>(
	state: &OraclePriceTracker<T>,
	chainlink_assetpair: ChainlinkAssetpair,
) -> Option<(Price, PriceStatus)>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice>,
{
	state
		.chain_states
		.get_latest_asset_state(chainlink_assetpair)
		.and_then(|asset_state| {
			Some((
				chainlink_price_to_statechain_price(
					&asset_state.price.median,
					chainlink_assetpair,
				)?
				.into(),
				asset_state.price_status,
			))
		})
}

pub fn chainlink_price_to_statechain_price(
	price: &ChainlinkPrice,
	assetpair: ChainlinkAssetpair,
) -> Option<StatechainPrice> {
	let from_unit = assetpair.to_price_unit();
	let to_unit = PriceUnit { base_asset: PriceAsset::Fine, quote_asset: PriceAsset::Fine };
	// WARNING: It is important that we first convert to the statechain representation,
	// and then do the unit conversion, because in the chainlink representation there aren't
	// enough decimals to represent "FineEth / FineUsd" prices
	// (1 Usd per Eth translates to 10^-12 FineUsd per FineEth)
	let price: StatechainPrice = price.clone().convert()?;
	let price: StatechainPrice = convert_unit(price, from_unit, to_unit)?;
	Some(price)
}
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct OraclePrice {
	pub price: Price,
	pub updated_at_oracle_timestamp: u64,
	pub updated_at_statechain_block: u32,
	pub base_asset: PriceAsset,
	pub quote_asset: PriceAsset,
}

// Used by the RPC
pub fn get_latest_oracle_prices<T>(
	state: &OraclePriceTracker<T>,
	base_and_quote_asset: Option<ChainlinkAssetpair>,
) -> Vec<OraclePrice>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice, StateChainBlockNumber = u32>,
{
	let pairs_iter = match base_and_quote_asset {
		Some(assetpair) => Either::Left(iter::once(assetpair)),
		None => Either::Right(all::<ChainlinkAssetpair>()),
	};
	pairs_iter
		.filter_map(|assetpair: ChainlinkAssetpair| {
			state.chain_states.get_latest_asset_state(assetpair).and_then(|asset_state| {
				let from_unit = assetpair.to_price_unit();

				let price: StatechainPrice =
					chainlink_price_to_statechain_price(&asset_state.price.median, assetpair)?;
				Some(OraclePrice {
					price: price.into(),
					updated_at_oracle_timestamp: asset_state.timestamp.median.seconds,
					updated_at_statechain_block: asset_state.updated_at_statechain_block,
					base_asset: from_unit.base_asset,
					quote_asset: from_unit.quote_asset,
				})
			})
		})
		.collect()
}
