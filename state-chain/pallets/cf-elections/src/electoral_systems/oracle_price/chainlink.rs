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

use cf_amm_math::Price;

use crate::electoral_systems::{
	oracle_price::{price::*, state_machine::*},
	state_machine::common_imports::*,
};

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
		.get_latest_price(chainlink_assetpair)
		.and_then(|(_, price, status)| {
			let from_unit = chainlink_assetpair.to_price_unit();
			let to_unit = PriceUnit { base_asset: PriceAsset::Fine, quote_asset: PriceAsset::Fine };
			let price: ChainlinkPrice = convert_unit(price, from_unit, to_unit)?;
			let price: StatechainPrice = price.convert()?;
			Some((price.into(), status))
		})
}
