// Copyright 2026 Chainflip Labs GmbH
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

use cf_utilities::migrations::{basics::HasVersion, v20300};
use pallet_cf_elections::electoral_systems::oracle_price::{
	chainlink::{OraclePrice as CurrentOraclePrice, OraclePriceLegacy as CurrentOraclePriceLegacy},
	price::PriceAsset as CurrentPriceAsset,
};

pub type PriceAsset = <CurrentPriceAsset as HasVersion<v20300>>::HistoricalType;
pub type OraclePrice = <CurrentOraclePrice as HasVersion<v20300>>::HistoricalType;
// The API v11 boundary predates the earliest canonical changelog snapshot. The distinct legacy
// carrier models the missing price_status; v20300 supplies its pre-v22 nested asset shape.
pub type OraclePriceLegacy = <CurrentOraclePriceLegacy as HasVersion<v20300>>::HistoricalType;

#[cfg(test)]
mod tests {
	use super::*;
	use cf_amm::math::Price;
	use cf_utilities::migrations::basics::{
		migrate_from_historical_type, try_migrate_to_historical_type,
	};
	use codec::Encode;
	use pallet_cf_elections::electoral_systems::oracle_price::state_machine::PriceStatus;

	#[test]
	fn historical_price_asset_encoding_matches_pre_v22_variants() {
		for current in [
			CurrentPriceAsset::Btc,
			CurrentPriceAsset::Eth,
			CurrentPriceAsset::Sol,
			CurrentPriceAsset::Usdc,
			CurrentPriceAsset::Usdt,
			CurrentPriceAsset::Usd,
			CurrentPriceAsset::Fine,
		] {
			let Ok(historical) = try_migrate_to_historical_type(v20300, current) else {
				panic!("pre-v22 price asset must migrate backwards")
			};
			assert_eq!(historical.encode(), current.encode());
			let migrated: CurrentPriceAsset = migrate_from_historical_type(v20300, historical);
			assert_eq!(migrated, current);
		}

		for current in [
			CurrentPriceAsset::Wbtc,
			CurrentPriceAsset::Cbbtc,
			CurrentPriceAsset::Dot,
			CurrentPriceAsset::Trx,
			CurrentPriceAsset::Bnb,
		] {
			assert!(try_migrate_to_historical_type(v20300, current).is_err());
		}
	}

	#[test]
	fn historical_oracle_price_encoding_matches_pre_v22_shape() {
		let current = CurrentOraclePrice {
			price: Price::from_raw(42u64.into()),
			updated_at_oracle_timestamp: 43,
			updated_at_statechain_block: 44,
			base_asset: CurrentPriceAsset::Btc,
			quote_asset: CurrentPriceAsset::Usd,
			price_status: PriceStatus::MaybeStale,
		};
		let Ok(historical) = try_migrate_to_historical_type(v20300, current.clone()) else {
			panic!("pre-v22 oracle price must migrate backwards")
		};

		assert_eq!(historical.encode(), current.encode());
		let migrated: CurrentOraclePrice = migrate_from_historical_type(v20300, historical);
		assert_eq!(migrated.encode(), current.encode());
	}

	#[test]
	fn historical_legacy_oracle_price_encoding_matches_pre_v11_shape() {
		let current = CurrentOraclePriceLegacy {
			price: Price::from_raw(42u64.into()),
			updated_at_oracle_timestamp: 43,
			updated_at_statechain_block: 44,
			base_asset: CurrentPriceAsset::Btc,
			quote_asset: CurrentPriceAsset::Usd,
		};
		let Ok(historical) = try_migrate_to_historical_type(v20300, current.clone()) else {
			panic!("pre-v11 oracle price must migrate backwards")
		};

		assert_eq!(historical.encode(), current.encode());
		let migrated: CurrentOraclePriceLegacy = migrate_from_historical_type(v20300, historical);
		let converted: CurrentOraclePrice = migrated.into();
		assert_eq!(converted.price_status, PriceStatus::Stale);
		assert_eq!(
			converted.encode(),
			CurrentOraclePrice {
				price: current.price,
				updated_at_oracle_timestamp: current.updated_at_oracle_timestamp,
				updated_at_statechain_block: current.updated_at_statechain_block,
				base_asset: current.base_asset,
				quote_asset: current.quote_asset,
				price_status: PriceStatus::Stale,
			}
			.encode()
		);
	}
}
