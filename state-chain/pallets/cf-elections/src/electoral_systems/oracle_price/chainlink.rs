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
use cf_utilities::macros::*;
use sp_std::iter;

derive_common_traits! {
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

impl ChainlinkAssetpair {
	fn from_price_assets(
		base_asset: PriceAsset,
		quote_asset: PriceAsset,
	) -> Option<ChainlinkAssetpair> {
		match (base_asset, quote_asset) {
			(PriceAsset::Btc, PriceAsset::Usd) => Some(ChainlinkAssetpair::BtcUsd),
			(PriceAsset::Eth, PriceAsset::Usd) => Some(ChainlinkAssetpair::EthUsd),
			(PriceAsset::Sol, PriceAsset::Usd) => Some(ChainlinkAssetpair::SolUsd),
			(PriceAsset::Usdc, PriceAsset::Usd) => Some(ChainlinkAssetpair::UsdcUsd),
			(PriceAsset::Usdt, PriceAsset::Usd) => Some(ChainlinkAssetpair::UsdtUsd),
			_ => None,
		}
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

#[cfg(any(feature = "runtime-benchmarks", test))]
pub fn statechain_price_to_chainlink_price(
	price: &StatechainPrice,
	assetpair: ChainlinkAssetpair,
) -> Option<ChainlinkPrice> {
	let from_unit = PriceUnit { base_asset: PriceAsset::Fine, quote_asset: PriceAsset::Fine };
	let to_unit = assetpair.to_price_unit();
	// WARNING: It is important that we first do the unit conversion,
	// and then convert to chainlink prices, because in the chainlink representation there aren't
	// enough decimals to represent "FineEth / FineUsd" prices
	// (1 Usd per Eth translates to 10^-12 FineUsd per FineEth)
	let price: StatechainPrice = convert_unit(price.clone(), from_unit, to_unit)?;
	let price: ChainlinkPrice = price.clone().convert()?;
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
	base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,
) -> Vec<OraclePrice>
where
	T: OPTypes<AssetPair = ChainlinkAssetpair, Price = ChainlinkPrice, StateChainBlockNumber = u32>,
{
	match base_and_quote_asset {
		Some(base_and_quote_asset) => {
			if let Some(assetpair) = ChainlinkAssetpair::from_price_assets(
				base_and_quote_asset.0,
				base_and_quote_asset.1,
			) {
				Either::Left(iter::once(assetpair))
			} else {
				return Vec::new();
			}
		},
		None => Either::Right(all::<ChainlinkAssetpair>()),
	}
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

#[cfg(test)]
mod tests {
	use super::*;
	use cf_amm_math::mul_div_floor_checked;
	use core::ops::Sub;
	use sp_core::U256;

	#[test]
	#[expect(clippy::inconsistent_digit_grouping)]
	#[expect(clippy::identity_op)]
	fn test_price_conversion() {
		// chainlink prices have 8 decimals
		assert_eq!(ChainlinkPrice::denominator(), U256::from(100_000_000u128));

		// the denominator of statechain prices is 2^128
		let internal_denom = U256::from(u128::MAX) + U256::one();

		// ------ btc --------

		// the price is 123456.7
		let price: ChainlinkPrice = Fraction::from_raw(U256::from(123456_70_000_000u128));

		// let's say the price is for Btc, convert it to our internal price
		let internal_price: StatechainPrice =
			chainlink_price_to_statechain_price(&price, ChainlinkAssetpair::BtcUsd).unwrap();

		// Also checking the reverse conversion (with small rounding error)
		assert_eq!(
			statechain_price_to_chainlink_price(&internal_price, ChainlinkAssetpair::BtcUsd)
				.unwrap(),
			price.sub(ChainlinkPrice::from_raw(U256::one()))
		);

		// now let's check whether we got the right result:
		//
		// the internal price is in FineUsd/FineBtc = 10^2 Usd/Btc, so translating from
		// Usd/Btc to FineUsd/FineBtc we have to divide by 10^2.
		//
		// fine_price = 1234.567 FineUsd/FineBtc
		//
		// In the internal representation, the upper 128 bits are the integral part,
		// the lower 128 bits are the fractional part.
		assert_eq!(
			internal_price.0,
			mul_div_floor_checked(U256::from(1234567), internal_denom, 10u128.pow(1 + 2)).unwrap()
		);

		// ------ eth --------

		let price: ChainlinkPrice = Fraction::from_raw(U256::from(3014_56_000_000u128));
		let internal_price =
			chainlink_price_to_statechain_price(&price, ChainlinkAssetpair::EthUsd).unwrap();

		// eth has 18 decimals, so these are 12 more than usd. Thus the conversion factor to convert
		// the price to a "fine price" is diving by 10^12 (and furthermore dividing by 10^2 due to
		// the 2 input decimals).
		assert_eq!(
			internal_price.0,
			mul_div_floor_checked(U256::from(3014_56u128), internal_denom, 10u128.pow(2 + 12))
				.unwrap()
		);

		// ------ sol --------

		let price: ChainlinkPrice = Fraction::from_raw(U256::from(235_89_000_000u128));
		let internal_price =
			chainlink_price_to_statechain_price(&price, ChainlinkAssetpair::SolUsd).unwrap();

		// sol has 9 decimals, so these are 3 more than usd. Thus the conversion factor to convert
		// the price to a "fine price" is diving by 10^3 (and furthermore dividing by 10^2 due to
		// the 2 input decimals).
		assert_eq!(
			internal_price.0,
			mul_div_floor_checked(U256::from(235_89u128), internal_denom, 10u128.pow(2 + 3))
				.unwrap()
		);

		// ------ usdc --------

		let price: ChainlinkPrice = Fraction::from_raw(U256::from(1_11_111_000u128));
		let internal_price =
			chainlink_price_to_statechain_price(&price, ChainlinkAssetpair::UsdcUsd).unwrap();

		// usdc has 6 decimals, this is the same as usd, so there's no conversion required.
		// the given price is 1.11111 (with 5 decimals), so we'll have to divide by 10^5
		assert_eq!(
			internal_price.0,
			mul_div_floor_checked(U256::from(1_11111u128), internal_denom, 10u128.pow(0 + 5))
				.unwrap()
		);

		// ------ usdt --------

		let price: ChainlinkPrice = Fraction::from_raw(U256::from(2_55_550_000u128));
		let internal_price =
			chainlink_price_to_statechain_price(&price, ChainlinkAssetpair::UsdcUsd).unwrap();

		// usdt has 6 decimals, this is the same as usd, so there's no conversion required.
		// the given price is 2.555 (with 4 decimals), so we'll have to divide by 10^4
		assert_eq!(
			internal_price.0,
			mul_div_floor_checked(U256::from(2_5555u128), internal_denom, 10u128.pow(0 + 4))
				.unwrap()
		);

		// Also checking the reverse conversion (with small rounding error)
		assert_eq!(
			statechain_price_to_chainlink_price(&internal_price, ChainlinkAssetpair::UsdcUsd)
				.unwrap(),
			price.sub(ChainlinkPrice::from_raw(U256::one()))
		);
	}
}
