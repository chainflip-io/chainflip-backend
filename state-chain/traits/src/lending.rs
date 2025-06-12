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

use cf_primitives::{Asset, AssetAmount, BasisPoints, BoostPoolTier, PrewitnessedDepositId};
use sp_std::collections::btree_map::BTreeMap;

use frame_support::pallet_prelude::DispatchError;

#[derive(Debug)]
pub struct BoostOutcome {
	pub used_pools: BTreeMap<BoostPoolTier, AssetAmount>,
	pub total_fee: AssetAmount,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct BoostFinalisationOutcome {
	pub network_fee: AssetAmount,
}

pub trait BoostApi {
	fn try_boosting(
		deposit_id: PrewitnessedDepositId,
		asset: Asset,
		deposit_amount: AssetAmount,
		max_boost_fee_bps: BasisPoints,
	) -> Result<BoostOutcome, DispatchError>;

	fn finalise_boost(deposit_id: PrewitnessedDepositId, asset: Asset) -> BoostFinalisationOutcome;

	fn process_deposit_as_lost(deposit_id: PrewitnessedDepositId, asset: Asset);
}
