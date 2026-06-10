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
use crate::LpStatsApi;
use cf_primitives::{Asset, AssetAmount};
use sp_runtime::{traits::Zero, FixedU128, Saturating};

use super::{MockPallet, MockPalletStorage};

type AccountId = u64;

pub struct MockLpStatsApi;

impl MockPallet for MockLpStatsApi {
	const PREFIX: &'static [u8] = b"MockLpStatsApi";
}

const LP_DELTA_USD_VOLUME: &[u8] = b"LP_DELTA_USD_VOLUME";

impl LpStatsApi for MockLpStatsApi {
	type AccountId = AccountId;

	fn on_limit_order_filled(lp: &Self::AccountId, asset: &Asset, usd_amount: AssetAmount) {
		Self::mutate_storage::<(AccountId, Asset), _, _, _, _>(
			LP_DELTA_USD_VOLUME,
			&(lp, asset),
			|delta| {
				let delta = delta.get_or_insert(FixedU128::zero());

				*delta = delta.saturating_add(FixedU128::from_inner(usd_amount));
			},
		);
	}
}
