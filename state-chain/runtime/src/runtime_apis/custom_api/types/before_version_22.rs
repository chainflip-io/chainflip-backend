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

use super::*;

#[derive(
	Encode, Decode, TypeInfo, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct EmissionsSafeMode {
	pub emissions_sync_enabled: bool,
}

impl Default for EmissionsSafeMode {
	fn default() -> Self {
		Self { emissions_sync_enabled: true }
	}
}

// The v20 ValidatorInfo, with apy_bp: emissions (and therefore APY) no longer exist as of v21.
#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct ValidatorInfo {
	pub balance: AssetAmount,
	pub bond: AssetAmount,
	pub last_heartbeat: u32,
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	#[deprecated]
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EvmAddress>,
	pub apy_bp: Option<u32>,
	pub restricted_balances: BTreeMap<EvmAddress, AssetAmount>,
	pub estimated_redeemable_balance: AssetAmount,
	pub operator: Option<AccountId32>,
	pub bid: AssetAmount,
	pub max_bid: Option<AssetAmount>,
}

impl From<ValidatorInfo> for super::ValidatorInfo {
	fn from(old: ValidatorInfo) -> Self {
		Self {
			balance: old.balance,
			bond: old.bond,
			last_heartbeat: old.last_heartbeat,
			reputation_points: old.reputation_points,
			keyholder_epochs: old.keyholder_epochs,
			is_current_authority: old.is_current_authority,
			#[expect(deprecated)]
			is_current_backup: old.is_current_backup,
			is_qualified: old.is_qualified,
			is_online: old.is_online,
			is_bidding: old.is_bidding,
			bound_redeem_address: old.bound_redeem_address,
			restricted_balances: old.restricted_balances,
			estimated_redeemable_balance: old.estimated_redeemable_balance,
			operator: old.operator,
			bid: old.bid,
			max_bid: old.max_bid,
		}
	}
}
