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
	AssetWithholding,
};
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::Saturating;

pub struct MockAssetWithholding;

impl MockPallet for MockAssetWithholding {
	const PREFIX: &'static [u8] = b"MockAssetWithholding";
}

impl MockAssetWithholding {
	pub fn withheld_assets(asset: Asset) -> AssetAmount {
		Self::get_storage::<Asset, AssetAmount>(WITHHELD_ASSETS, asset).unwrap_or_default()
	}
}

const WITHHELD_ASSETS: &[u8] = b"WITHHELD_ASSETS";

impl AssetWithholding for MockAssetWithholding {
	fn withhold_assets(asset: Asset, amount: AssetAmount) {
		Self::mutate_storage::<Asset, _, AssetAmount, _, _>(
			WITHHELD_ASSETS,
			&asset,
			|value: &mut Option<AssetAmount>| {
				value.get_or_insert_default().saturating_accrue(amount);
			},
		);
	}
}
