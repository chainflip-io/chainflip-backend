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

use sp_std::collections::btree_map::BTreeMap;

use crate::{
	lending::{BoostApi, BoostFinalisationOutcome, BoostOutcome, LendingSystemApi},
	LendingSwapType,
};

use cf_primitives::{Asset, AssetAmount, BasisPoints, PrewitnessedDepositId, SwapRequestId};
use frame_support::{pallet_prelude::*, sp_runtime::Percent};

use super::{MockPallet, MockPalletStorage};

pub struct MockBoostApi {}

impl MockPallet for MockBoostApi {
	const PREFIX: &'static [u8] = b"MockBoostApi";
}

const AVAILABLE_FUNDS: &[u8] = b"AVAILABLE_FUNDS";
const NETWORK_FEE: &[u8] = b"NETWORK_FEE";
const BOOSTED_DEPOSITS: &[u8] = b"BOOSTED_DEPOSITS";

#[derive(Decode, Encode, Debug, PartialEq, Eq)]
struct BoostAmounts {
	owed_amount: AssetAmount,
	network_fee: AssetAmount,
}

type BoostedDeposits = BTreeMap<PrewitnessedDepositId, BoostAmounts>;

impl MockBoostApi {
	pub fn set_available_amount(amount: AssetAmount) {
		Self::put_value(AVAILABLE_FUNDS, amount);
	}

	pub fn get_available_amount() -> AssetAmount {
		Self::get_value(AVAILABLE_FUNDS).unwrap_or_default()
	}

	pub fn set_network_fee_percent(percent: Percent) {
		Self::put_value(NETWORK_FEE, percent);
	}

	pub fn get_network_fee_percent() -> Percent {
		Self::get_value(NETWORK_FEE).unwrap_or_default()
	}

	pub fn is_deposit_boosted(deposit_id: PrewitnessedDepositId) -> bool {
		let boosted_deposits =
			Self::get_value::<BoostedDeposits>(BOOSTED_DEPOSITS).unwrap_or_default();

		boosted_deposits.contains_key(&deposit_id)
	}

	fn add_boosted_deposit(deposit_id: PrewitnessedDepositId, amounts: BoostAmounts) {
		let mut boosted_deposits =
			Self::get_value::<BoostedDeposits>(BOOSTED_DEPOSITS).unwrap_or_default();

		assert_eq!(
			boosted_deposits.insert(deposit_id, amounts),
			None,
			"deposit was already boosted"
		);

		Self::put_value(BOOSTED_DEPOSITS, boosted_deposits);
	}

	fn remove_boosted_deposit(deposit_id: PrewitnessedDepositId) -> BoostAmounts {
		let mut boosted_deposits =
			Self::get_value::<BoostedDeposits>(BOOSTED_DEPOSITS).unwrap_or_default();

		let deposit_amount =
			boosted_deposits.remove(&deposit_id).expect("deposit must have been boosted");

		Self::put_value(BOOSTED_DEPOSITS, boosted_deposits);

		deposit_amount
	}
}

impl BoostApi for MockBoostApi {
	fn try_boosting(
		deposit_id: PrewitnessedDepositId,
		_asset: Asset,
		deposit_amount: AssetAmount,
		max_boost_fee_bps: BasisPoints,
	) -> Result<BoostOutcome, DispatchError> {
		// The mock assumes there is only one pool
		let total_fee = deposit_amount * max_boost_fee_bps as u128 / 10_000;

		let available_amount = Self::get_available_amount();

		let required_amount = deposit_amount - total_fee;

		if available_amount < required_amount {
			return Err("insufficient liquidity".into());
		}

		let network_fee = Self::get_network_fee_percent() * total_fee;

		Self::add_boosted_deposit(
			deposit_id,
			BoostAmounts { owed_amount: deposit_amount - network_fee, network_fee },
		);

		Self::set_available_amount(available_amount - required_amount);

		let used_pools = BTreeMap::from_iter([(max_boost_fee_bps, deposit_amount)]);

		Ok(BoostOutcome { used_pools, total_fee })
	}

	fn finalise_boost(
		deposit_id: PrewitnessedDepositId,
		_asset: Asset,
	) -> BoostFinalisationOutcome {
		let BoostAmounts { owed_amount, network_fee } = Self::remove_boosted_deposit(deposit_id);
		Self::set_available_amount(Self::get_available_amount() + owed_amount);
		BoostFinalisationOutcome { network_fee }
	}

	fn process_deposit_as_lost(deposit_id: PrewitnessedDepositId, _asset: Asset) {
		let _deposit_amount = Self::remove_boosted_deposit(deposit_id);
	}
}

pub struct MockLendingSystemApi {}

impl MockPallet for MockLendingSystemApi {
	const PREFIX: &'static [u8] = b"MockLendingSystemApi";
}

const SWAPPED_FEES: &[u8] = b"SWAPPED_FEES";

impl MockLendingSystemApi {
	pub fn set_swapped_fees(asset: Asset, amount: AssetAmount) {
		Self::put_storage(SWAPPED_FEES, asset, amount);
	}

	pub fn get_swapped_fees(asset: Asset) -> Option<AssetAmount> {
		Self::get_storage(SWAPPED_FEES, asset)
	}
}

impl LendingSystemApi for MockLendingSystemApi {
	type AccountId = u64;

	fn process_loan_swap_outcome(
		_swap_request_id: SwapRequestId,
		swap_type: LendingSwapType<Self::AccountId>,
		output_amount: AssetAmount,
	) {
		match swap_type {
			LendingSwapType::Liquidation { .. } => {
				// TODO: implement if needed by some test
			},
			LendingSwapType::FeeSwap { pool_asset } => {
				let current_fees = Self::get_swapped_fees(pool_asset).unwrap_or_default();
				Self::set_swapped_fees(pool_asset, current_fees + output_amount);
			},
		}
	}
}
