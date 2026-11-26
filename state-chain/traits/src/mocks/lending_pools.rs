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
	lending::{
		BoostApi, BoostFinalisationOutcome, BoostOutcome, BoostSource, LendingSystemApi, LoanId,
	},
	LendingSwapType,
};

use cf_amm::math::Amount;
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, OrderId, PrewitnessedDepositId, SwapRequestId,
};
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

		Ok(BoostOutcome {
			amounts: [(BoostSource::LendingPool, deposit_amount)].into_iter().collect(),
			fees: [(BoostSource::LendingPool, total_fee)].into_iter().collect(),
		})
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

const LIQUIDATION_SWAP_OUTCOMES: &[u8] = b"LIQUIDATION_SWAP_OUTCOMES";

#[derive(Encode, Decode, PartialEq, Eq, Debug)]
pub struct LiquidationSwapOutcome {
	pub borrower_id: u64,
	pub loan_id: LoanId,
	pub output_amount: AssetAmount,
}

impl MockLendingSystemApi {
	pub fn set_liquidation_swap_outcome(
		swap_request_id: SwapRequestId,
		outcome: LiquidationSwapOutcome,
	) {
		Self::put_storage(LIQUIDATION_SWAP_OUTCOMES, swap_request_id, outcome);
	}

	pub fn get_liquidation_swap_outcome(
		swap_request_id: SwapRequestId,
	) -> Option<LiquidationSwapOutcome> {
		Self::get_storage(LIQUIDATION_SWAP_OUTCOMES, swap_request_id)
	}
}

impl LendingSystemApi for MockLendingSystemApi {
	type AccountId = u64;

	fn process_loan_swap_outcome(
		swap_request_id: SwapRequestId,
		swap_type: LendingSwapType<Self::AccountId>,
		output_amount: AssetAmount,
	) {
		match swap_type {
			LendingSwapType::Liquidation { borrower_id, loan_id } =>
				Self::set_liquidation_swap_outcome(
					swap_request_id,
					LiquidationSwapOutcome { borrower_id, loan_id, output_amount },
				),
		}
	}
}

/// A minimal stand-in for the lending pool: a supply position and a promised amount per
/// (account, asset), plus a pool-wide cap on what a swap can draw. Interest, borrowing and LTV
/// are not modelled — tests set the caps directly.
pub struct MockLendingOrderApi<AccountId = u64>(sp_std::marker::PhantomData<AccountId>);

impl<AccountId> MockPallet for MockLendingOrderApi<AccountId> {
	const PREFIX: &'static [u8] = b"MOCK_LENDING_SUPPLY";
}

const SUPPLY_POSITION: &[u8] = b"SUPPLY_POSITION";
const COMMITMENTS: &[u8] = b"COMMITMENTS";
const POOL_LIQUIDITY: &[u8] = b"POOL_LIQUIDITY";

impl<AccountId: codec::Encode + codec::Decode + Ord + Clone> MockLendingOrderApi<AccountId> {
	/// Gives `who` a supply position to open orders against.
	pub fn add_supply(who: &AccountId, asset: Asset, amount: AssetAmount) {
		Self::mutate_storage::<(&AccountId, Asset), _, _, _, _>(
			SUPPLY_POSITION,
			&(who, asset),
			|position| {
				let position: &mut AssetAmount = position.get_or_insert_with(|| 0);
				*position = position.saturating_add(amount);
			},
		);
	}

	pub fn supply_position(who: &AccountId, asset: Asset) -> AssetAmount {
		Self::get_storage(SUPPLY_POSITION, (who, asset)).unwrap_or_default()
	}

	/// How much of `who`'s position is promised to open orders.
	pub fn committed(who: &AccountId, asset: Asset) -> AssetAmount {
		Self::commitments(asset).get(who).copied().unwrap_or_default()
	}

	/// Caps what the pool will hand over to fills, standing in for supply that has been lent out.
	pub fn set_pool_liquidity(asset: Asset, amount: AssetAmount) {
		Self::put_storage(POOL_LIQUIDITY, asset, amount);
	}

	fn commitments(asset: Asset) -> BTreeMap<AccountId, AssetAmount> {
		Self::get_storage(COMMITMENTS, asset).unwrap_or_default()
	}

	fn mutate_commitments(asset: Asset, f: impl FnOnce(&mut BTreeMap<AccountId, AssetAmount>)) {
		Self::mutate_storage::<Asset, _, _, _, _>(COMMITMENTS, &asset, |commitments| {
			f(commitments.get_or_insert_with(BTreeMap::new))
		})
	}
}

impl<AccountId: codec::Encode + codec::Decode + Ord + Clone> crate::LendingOrderApi
	for MockLendingOrderApi<AccountId>
{
	type AccountId = AccountId;

	type OrderBudget = MockOrderBudget<AccountId>;

	fn order_budget(asset: Asset) -> Self::OrderBudget {
		MockOrderBudget {
			pool_liquidity: Self::get_storage(POOL_LIQUIDITY, asset).unwrap_or(AssetAmount::MAX),
			available: Self::commitments(asset),
		}
	}

	fn commit_supply_to_order(
		lp: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::sp_runtime::DispatchResult {
		let position = Self::supply_position(lp, asset);
		let committed = Self::committed(lp, asset).saturating_add(amount);
		if committed > position {
			return Err(frame_support::sp_runtime::DispatchError::Other("Insufficient supply"));
		}

		Self::mutate_commitments(asset, |commitments| {
			commitments.insert(lp.clone(), committed);
		});

		Ok(())
	}

	fn release_order_commitment(lp: &Self::AccountId, asset: Asset, amount: AssetAmount) {
		Self::mutate_commitments(asset, |commitments| {
			if let Some(committed) = commitments.get_mut(lp) {
				*committed = committed.saturating_sub(amount);
				if *committed == 0 {
					commitments.remove(lp);
				}
			}
		})
	}

	fn withdraw_for_order_fill(
		lp: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::sp_runtime::DispatchResult {
		if Self::committed(lp, asset) < amount || Self::supply_position(lp, asset) < amount {
			return Err(frame_support::sp_runtime::DispatchError::Other("Insufficient liquidity"));
		}

		Self::mutate_storage::<(&AccountId, Asset), _, _, _, _>(
			SUPPLY_POSITION,
			&(lp, asset),
			|position| {
				let position: &mut AssetAmount = position.get_or_insert_with(|| 0);
				*position = position.saturating_sub(amount);
			},
		);
		Self::release_order_commitment(lp, asset, amount);

		Ok(())
	}
}

/// The mock's view of a pool for one swap. Commitments stand in for the real per-lender limit;
/// LTV is not modelled.
pub struct MockOrderBudget<AccountId> {
	pool_liquidity: AssetAmount,
	available: BTreeMap<AccountId, AssetAmount>,
}

impl<AccountId: Ord> cf_amm::common::SupplyBudget<(AccountId, OrderId)>
	for MockOrderBudget<AccountId>
{
	fn allocate<'a>(
		&self,
		claims: &[(&'a (AccountId, OrderId), Amount)],
	) -> BTreeMap<&'a (AccountId, OrderId), Amount> {
		cf_amm::common::allocate_per_account(claims, |lp| {
			self.available.get(lp).copied().unwrap_or_default().into()
		})
	}

	fn total_available(&self) -> Amount {
		self.pool_liquidity.into()
	}

	fn take(&mut self, (lp, _id): &(AccountId, OrderId), amount: Amount) {
		let amount = amount.try_into().unwrap_or(AssetAmount::MAX);
		if let Some(available) = self.available.get_mut(lp) {
			*available = available.saturating_sub(amount);
		}
		self.pool_liquidity = self.pool_liquidity.saturating_sub(amount);
	}
}
