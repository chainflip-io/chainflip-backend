// The `RealisticCallParams` type is used in a runtime call and has to be available when building
// with `doc` configuration.
#![cfg(any(feature = "runtime-benchmarks", doc))]
#![allow(dead_code)]

use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_std::collections::btree_map::BTreeMap;

/// Representative benchmark type modeled after real pallet call parameters.
/// Based on `request_loan` from cf-lending-pools which has typical complexity.
/// Parameters: asset enum, amount (u128), BTreeMap<Asset, Amount>
#[derive(TypeInfo, Clone, Encode, Decode, DecodeWithMemTracking, Debug, PartialEq, Eq)]
pub struct RealisticCallParams {
	pub loan_asset: Asset,
	pub loan_amount: AssetAmount,
	pub extra_collateral: BTreeMap<Asset, AssetAmount>,
}

impl Default for RealisticCallParams {
	fn default() -> Self {
		{
			let mut extra_collateral = BTreeMap::new();
			extra_collateral.insert(Asset::Eth, 1_000_000_000_000_000_000u128);
			extra_collateral.insert(Asset::Usdc, 50_000_000_000u128);

			RealisticCallParams {
				loan_asset: Asset::Usdc,
				loan_amount: 100_000_000_000u128,
				extra_collateral,
			}
		}
	}
}
