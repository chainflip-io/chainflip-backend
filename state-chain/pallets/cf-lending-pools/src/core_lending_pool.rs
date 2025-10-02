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

#[cfg(test)]
mod tests;

use cf_primitives::{define_wrapper_type, AssetAmount};
use cf_runtime_utilities::log_or_panic;
use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::{
		helpers_128bit::multiply_by_rational_with_rounding, PerThing, Perquintill, Rounding,
	},
	DefaultNoBound,
};
use nanorand::{Rng, WyRand};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use crate::LoanUsage;

mod scaled_amount {

	use super::*;

	use cf_primitives::AssetAmount;
	use frame_support::sp_runtime::{traits::Saturating, SaturatedConversion};

	/// Represents 1/SCALE_FACTOR of Asset amount as a way to gain extra precision.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
	pub struct ScaledAmount<const SCALE_FACTOR: u128> {
		val: u128,
	}

	impl<const SCALE_FACTOR: u128> PartialOrd for ScaledAmount<SCALE_FACTOR> {
		fn partial_cmp(&self, other: &Self) -> Option<scale_info::prelude::cmp::Ordering> {
			self.val.partial_cmp(&other.val)
		}
	}

	impl<const SCALE_FACTOR: u128> Copy for ScaledAmount<SCALE_FACTOR> {}

	impl<const SCALE_FACTOR: u128> core::iter::Sum for ScaledAmount<SCALE_FACTOR> {
		fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
			iter.fold(ScaledAmount::default(), |acc, x| acc + x)
		}
	}

	impl<const SCALE_FACTOR: u128> From<ScaledAmount<SCALE_FACTOR>> for u128 {
		fn from(amount: ScaledAmount<SCALE_FACTOR>) -> Self {
			amount.val
		}
	}

	impl<const SCALE_FACTOR: u128> From<u128> for ScaledAmount<SCALE_FACTOR> {
		fn from(val: u128) -> Self {
			ScaledAmount { val }
		}
	}

	impl<const SCALE_FACTOR: u128> core::ops::Add<Self> for ScaledAmount<SCALE_FACTOR> {
		type Output = Self;

		fn add(self, rhs: Self) -> Self::Output {
			self.saturating_add(rhs)
		}
	}

	impl<const SCALE_FACTOR: u128> ScaledAmount<SCALE_FACTOR> {
		pub fn from_asset_amount(amount: AssetAmount) -> Self {
			let amount: u128 = amount.saturated_into();
			amount.saturating_mul(SCALE_FACTOR).into()
		}

		// Convenience method to create ScaledAmount from u128
		// without scaling
		pub const fn from_raw(val: u128) -> Self {
			ScaledAmount { val }
		}

		pub fn as_raw(&self) -> u128 {
			self.val
		}

		pub fn into_asset_amount(self) -> AssetAmount {
			self.val
				.checked_div(SCALE_FACTOR)
				.expect("Scale factor is not 0")
				.saturated_into()
		}

		/// Removes and returns the "whole" part leaving only the fractional part
		pub fn take_non_fractional_part(&mut self) -> AssetAmount {
			let amount_taken = self.into_asset_amount();

			self.saturating_reduce(Self::from_asset_amount(amount_taken));

			amount_taken
		}

		pub fn checked_sub(self, rhs: Self) -> Option<Self> {
			self.val.checked_sub(rhs.val).map(|val| val.into())
		}

		pub fn saturating_sub(self, rhs: Self) -> Self {
			self.val.saturating_sub(rhs.val).into()
		}

		#[cfg(test)]
		pub fn checked_add(self, rhs: Self) -> Option<Self> {
			self.val.checked_add(rhs.val).map(|val| val.into())
		}

		pub fn saturating_add(self, rhs: Self) -> Self {
			self.val.saturating_add(rhs.val).into()
		}

		pub fn saturating_accrue(&mut self, rhs: Self) {
			self.val.saturating_accrue(rhs.val)
		}

		pub fn saturating_reduce(&mut self, rhs: Self) {
			self.val.saturating_reduce(rhs.val)
		}
	}

	impl<const SCALE_FACTOR: u128> core::ops::Mul<Perquintill> for ScaledAmount<SCALE_FACTOR> {
		type Output = Self;

		fn mul(self, rhs: Perquintill) -> Self::Output {
			ScaledAmount::from_raw(rhs.mul_floor(self.as_raw()))
		}
	}
}

/// Low precision version of scaled amount that's sufficient for representing boost fees
/// (boost could also use ScaledAmountHP, but that would require migration)
type ScaledAmount = scaled_amount::ScaledAmount<1000>;

/// High precision version of scaled amount
pub type ScaledAmountHP = scaled_amount::ScaledAmount<1_000_000_000>;

define_wrapper_type!(CoreLoanId, u64, extra_derives: Ord, PartialOrd);

impl core::ops::Add<u64> for CoreLoanId {
	type Output = Self;

	fn add(self, rhs: u64) -> Self::Output {
		CoreLoanId(self.0 + rhs)
	}
}

type UnlockedFunds<AccountId> = Vec<(AccountId, AssetAmount)>;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct PendingLoan<AccountId> {
	pub usage: LoanUsage,
	pub shares: BTreeMap<AccountId, Perquintill>,
}

#[derive(Clone, Debug, DefaultNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct CoreLendingPool<AccountId> {
	pub next_loan_id: CoreLoanId,
	// Total available amount (not currently used in any loan)
	pub available_amount: ScaledAmount,
	// Mapping from LP to the available amount they own in `available_amount`
	pub amounts: BTreeMap<AccountId, ScaledAmount>,
	// Pending loans awaiting finalisation and how much of them is owed to which LP
	pub pending_loans: BTreeMap<CoreLoanId, PendingLoan<AccountId>>,
	// Stores LPs who have opted to stop lending, along with any pending loans awaiting
	// finalisation.
	pub pending_withdrawals: BTreeMap<AccountId, BTreeSet<CoreLoanId>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
	AccountNotFoundInPool,
}

impl<AccountId> CoreLendingPool<AccountId>
where
	AccountId: PartialEq + Ord + Clone + core::fmt::Debug,
	for<'a> &'a AccountId: PartialEq,
{
	pub fn add_funds(&mut self, lender_id: AccountId, amount: AssetAmount) {
		self.add_funds_inner(lender_id, ScaledAmount::from_asset_amount(amount));
	}

	pub fn stop_lending(
		&mut self,
		lender_id: AccountId,
	) -> Result<(AssetAmount, BTreeSet<LoanUsage>), Error> {
		let Some(lender_unlocked_amount) = self.amounts.remove(&lender_id) else {
			return Err(Error::AccountNotFoundInPool);
		};

		self.available_amount.saturating_reduce(lender_unlocked_amount);

		let (pending_loans, pending_loans_usage): (BTreeSet<_>, BTreeSet<_>) = self
			.pending_loans
			.iter()
			.filter(|(_, loan)| loan.shares.contains_key(&lender_id))
			.map(|(loan_id, loan)| (*loan_id, loan.usage.clone()))
			.unzip();

		if !pending_loans.is_empty() {
			self.pending_withdrawals.insert(lender_id, pending_loans.clone());
		}

		Ok((lender_unlocked_amount.into_asset_amount(), pending_loans_usage))
	}

	/// Attempt to use pool's available funds to create a loan of `amount_to_borrow`.
	pub fn new_loan(
		&mut self,
		amount_to_borrow: AssetAmount,
		usage: LoanUsage,
	) -> Result<CoreLoanId, &'static str> {
		let loan_id = self.next_loan_id;
		self.next_loan_id.0 += 1;

		let mut total_contributed = ScaledAmount::default();
		let amount_to_borrow = ScaledAmount::from_asset_amount(amount_to_borrow);

		let current_total_available_amount = self.available_amount;

		self.available_amount = self
			.available_amount
			.checked_sub(amount_to_borrow)
			.ok_or("Not enough available funds")?;

		let shares: BTreeMap<_, _> = self
			.amounts
			.iter_mut()
			.map(|(booster_id, lp_amount)| {
				let share = Perquintill::from_rational_with_rounding::<u128>(
					(*lp_amount).into(),
					current_total_available_amount.into(),
					// Round down to ensure the sum of shares does not exceed 1
					Rounding::Down,
				)
				.unwrap_or_default();

				// Round deducted amount up to ensure that rounding errors don't affect our
				// ability to contribute required amount (note that the result can never be
				// greater than boosters `amount` since we checked that required_amount <=
				// total_available_amount).
				// Note the we don't use share since we don't want the rounded down value.
				let lp_contribution = multiply_by_rational_with_rounding(
					amount_to_borrow.as_raw(),
					(*lp_amount).as_raw(),
					current_total_available_amount.into(),
					Rounding::Up,
				)
				// lender's amount is always <= total amount so default due to overflow should be
				// impossible
				.unwrap_or_default()
				.into();

				total_contributed.saturating_accrue(lp_contribution);
				lp_amount.saturating_reduce(lp_contribution);

				(booster_id.clone(), share)
			})
			.collect();

		// We may have contributed a tiny amount more than necessary.
		// This shouldn't saturate due to amounts to contribute being rounded up:
		let excess_contributed = total_contributed.saturating_sub(amount_to_borrow);

		// Some "lucky" lender may be credited some (inconsequential) amount back to
		// ensure that we correctly account for every single atomic unit even in presence
		// of rounding errors:
		let lucky_index = WyRand::new_seed(loan_id.0).generate_range(0..self.amounts.len());
		if let Some((_lp_id, amount)) = self.amounts.iter_mut().nth(lucky_index) {
			amount.saturating_accrue(excess_contributed);
		}

		self.pending_loans
			.try_insert(loan_id, PendingLoan { shares, usage })
			.map_err(|_| "Pending loan id already exists")?;

		Ok(loan_id)
	}

	pub fn make_repayment(
		&mut self,
		loan_id: CoreLoanId,
		repayment_amount: AssetAmount,
	) -> UnlockedFunds<AccountId> {
		let Some(PendingLoan { shares, .. }) = self.pending_loans.get(&loan_id) else {
			return Default::default();
		};

		let repayment_amount = ScaledAmount::from_asset_amount(repayment_amount);

		let mut unlocked_funds = vec![];

		let mut total_credited: ScaledAmount = 0.into();

		let amounts_to_credit = {
			let mut amounts_to_credit: BTreeMap<_, _> = shares
				.iter()
				.map(|(lp_id, share)| {
					let lp_amount = repayment_amount * (*share);
					total_credited = total_credited.saturating_add(lp_amount);

					(lp_id.clone(), lp_amount)
				})
				.collect();

			// We may still have some tiny amount of funds to credit due to rounding errors.
			// This shouldn't saturate due to the amount to receive being rounded down:
			let remaining_to_credit = repayment_amount.saturating_sub(total_credited);

			// Some "lucky" lender may receive some (inconsequential) amount to
			// ensure that we correctly account for every single atomic unit even in presence
			// of rounding errors:
			let lucky_index =
				WyRand::new_seed(loan_id.0).generate_range(0..amounts_to_credit.len());
			if let Some((_lp_id, amount)) = amounts_to_credit.iter_mut().nth(lucky_index) {
				amount.saturating_accrue(remaining_to_credit);
			}

			amounts_to_credit
		};

		for (lp_id, amount) in amounts_to_credit {
			// Depending on whether the lender is in the "stop lending" state, add deposits to
			// their free balance or back to the available boost pool:
			if self.pending_withdrawals.contains_key(&lp_id) {
				if amount > Default::default() {
					unlocked_funds.push((lp_id, amount.into_asset_amount()));
				}
			} else {
				self.add_funds_inner(lp_id, amount);
			}
		}

		unlocked_funds
	}

	pub fn finalise_loan(&mut self, loan_id: CoreLoanId) {
		let Some(PendingLoan { shares, .. }) = self.pending_loans.remove(&loan_id) else {
			return Default::default();
		};

		for lp_id in shares.keys() {
			if let Some(pending_loans) = self.pending_withdrawals.get_mut(lp_id) {
				if !pending_loans.remove(&loan_id) {
					log_or_panic!("Withdrawing lender contributed to loan {loan_id}, but it is not in pending withdrawals");
				}

				if pending_loans.is_empty() {
					self.pending_withdrawals.remove(lp_id);
				}
			}
		}
	}

	pub fn get_available_amount(&self) -> AssetAmount {
		self.available_amount.into_asset_amount()
	}

	pub fn get_amounts(&self) -> BTreeMap<AccountId, AssetAmount> {
		self.amounts
			.iter()
			.map(|(account_id, scaled_amount)| {
				(account_id.clone(), scaled_amount.into_asset_amount())
			})
			.collect()
	}

	pub fn get_pending_loans(&self) -> &BTreeMap<CoreLoanId, PendingLoan<AccountId>> {
		&self.pending_loans
	}

	fn add_funds_inner(&mut self, lender_id: AccountId, amount: ScaledAmount) {
		// To keep things simple, we assume that the booster no longer wants to withdraw
		// if they add more funds:
		self.pending_withdrawals.remove(&lender_id);

		self.amounts.entry(lender_id).or_default().saturating_accrue(amount);
		self.available_amount.saturating_accrue(amount);
	}

	pub fn get_available_amount_for_account(&self, lender_id: &AccountId) -> Option<AssetAmount> {
		self.amounts.get(lender_id).copied().map(|a| a.into_asset_amount())
	}
}
