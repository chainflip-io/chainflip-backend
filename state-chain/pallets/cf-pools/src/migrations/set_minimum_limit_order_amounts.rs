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

//! Seeds the initial per-asset minimum limit order amounts directly into storage.

use crate::{Config, MinimumLimitOrderAmount};
use cf_primitives::{Asset, AssetAmount};
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Initial minimum limit order amount per asset, in the asset's smallest unit (decimals noted
/// per row). The minimum applies to the asset being *sold* by an order
const MINIMUMS: [(Asset, AssetAmount); 20] = [
	(Asset::Eth, 3_000_000_000_000_000), // 0.003ETH, 18 decimals
	(Asset::Flip, 10_000_000_000_000_000_000), // 10FLIP, 18 decimals
	(Asset::Usdc, 5_000_000),            // 5USDC, 6 decimals
	(Asset::Usdt, 5_000_000),            // 5USDT, 6 decimals
	(Asset::Wbtc, 10000),                // 0.0001WBTC, 8 decimals
	(Asset::Dot, 50_000_000_000),        // 5DOT, 10 decimals
	(Asset::Btc, 10000),                 // 0.0001BTC, 8 decimals
	(Asset::ArbEth, 3_000_000_000_000_000), // 0.003ETH, 18 decimals
	(Asset::ArbUsdc, 5_000_000),         // 5ARBUSDC, 6 decimals
	(Asset::ArbUsdt, 5_000_000),         // 5ARBUSDT, 6 decimals
	(Asset::Sol, 100_000_000),           // 0.1SOL, 9 decimals
	(Asset::SolUsdc, 5_000_000),         // 5SOLUSDC, 6 decimals
	(Asset::SolUsdt, 5_000_000),         // 5SOLUSDT, 6 decimals
	(Asset::HubDot, 50_000_000_000),     // 5HUBDOT, 10 decimals
	(Asset::HubUsdt, 5_000_000),         // 5HUBUSDT, 6 decimals
	(Asset::HubUsdc, 5_000_000),         // 5HUBUSDC, 6 decimals
	(Asset::Trx, 15_000_000),            // 15TRX, 6 decimals
	(Asset::TrxUsdt, 5_000_000),         // 5TRXUSDT, 6 decimals
	(Asset::Bnb, 10_000_000_000_000_000), // 0.01BNB, 18 decimals
	(Asset::BscUsdt, 5_000_000_000_000_000_000), // 5BSCUSDT, 18 decimals
];

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		for (asset, amount) in MINIMUMS {
			MinimumLimitOrderAmount::<T>::insert(asset, amount);
		}
		T::DbWeight::get().writes(MINIMUMS.len() as u64)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		// Every asset must be covered, so a newly added asset can't be silently left out.
		for asset in Asset::all() {
			frame_support::ensure!(
				MINIMUMS.iter().any(|(a, _)| *a == asset),
				"MINIMUMS is missing an Asset variant"
			);
		}
		for (asset, amount) in MINIMUMS {
			frame_support::ensure!(
				MinimumLimitOrderAmount::<T>::get(asset) == amount,
				"MinimumLimitOrderAmount was not seeded as expected"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	#[test]
	fn seeds_minimums_for_every_asset() {
		new_test_ext().execute_with(|| {
			Migration::<Test>::on_runtime_upgrade();

			// The table covers every asset...
			for asset in Asset::all() {
				assert!(
					MINIMUMS.iter().any(|(a, _)| *a == asset),
					"{asset:?} missing from MINIMUMS"
				);
			}
			// ...and each configured value is written to storage.
			for (asset, amount) in MINIMUMS {
				assert_eq!(MinimumLimitOrderAmount::<Test>::get(asset), amount);
			}
		});
	}
}
