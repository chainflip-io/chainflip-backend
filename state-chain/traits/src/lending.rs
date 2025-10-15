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

use cf_primitives::{
	define_wrapper_type, Asset, AssetAmount, BasisPoints, BoostPoolTier, PrewitnessedDepositId,
	SwapRequestId,
};
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

use frame_support::pallet_prelude::DispatchError;

use crate::LendingSwapType;

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

define_wrapper_type!(LoanId, u64, extra_derives: PartialOrd, Ord, Serialize, Deserialize);

impl core::ops::Add<u64> for LoanId {
	type Output = Self;

	fn add(self, rhs: u64) -> Self::Output {
		LoanId(self.0 + rhs)
	}
}

pub trait LendingApi {
	type AccountId;

	fn expand_loan(
		borrower: Self::AccountId,
		loan_id: LoanId,
		extra_amount_to_borrow: AssetAmount,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError>;

	fn new_loan(
		borrower: Self::AccountId,
		asset: Asset,
		amount_to_borrow: AssetAmount,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<LoanId, DispatchError>;

	fn try_making_repayment(
		borrower_id: &Self::AccountId,
		loan_id: LoanId,
		amount: AssetAmount,
	) -> Result<(), DispatchError>;

	fn add_collateral(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError>;

	fn remove_collateral(
		borrower_id: &Self::AccountId,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError>;
}

pub trait LendingSystemApi {
	type AccountId;

	fn process_loan_swap_outcome(
		swap_request_id: SwapRequestId,
		swap_type: LendingSwapType<Self::AccountId>,
		output_amount: AssetAmount,
	);
}
