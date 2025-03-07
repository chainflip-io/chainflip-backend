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
	mocks::{MockPallet, MockPalletStorage},
	LiabilityTracker,
};
use cf_chains::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::Saturating;
use sp_std::collections::btree_map::BTreeMap;

pub struct MockLiabilityTracker;

impl MockPallet for MockLiabilityTracker {
	const PREFIX: &'static [u8] = b"MockLiabilityTracker";
}

impl MockLiabilityTracker {
	pub fn total_liabilities(asset: Asset) -> AssetAmount {
		Self::get_storage::<Asset, BTreeMap<ForeignChainAddress, AssetAmount>>(LIABILITIES, asset)
			.unwrap_or_default()
			.values()
			.sum::<AssetAmount>()
	}
}

const LIABILITIES: &[u8] = b"LIABILITIES";

impl LiabilityTracker for MockLiabilityTracker {
	fn record_liability(account_id: ForeignChainAddress, asset: Asset, amount: AssetAmount) {
		Self::mutate_storage::<Asset, _, BTreeMap<ForeignChainAddress, AssetAmount>, _, _>(
			LIABILITIES,
			&asset,
			|value: &mut Option<BTreeMap<ForeignChainAddress, AssetAmount>>| {
				value
					.get_or_insert_default()
					.entry(account_id)
					.or_default()
					.saturating_accrue(amount);
			},
		);
	}
}
