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

use crate::{AccountInfo, Chainflip, FundAccount, FundingInfo, FundingSource};
use frame_support::{
	sp_runtime::{
		traits::{CheckedSub, Zero},
		Saturating,
	},
	Never,
};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use super::{MockPallet, MockPalletStorage};

pub struct MockFundingInfo<T>(PhantomData<T>);

impl<T> MockPallet for MockFundingInfo<T> {
	const PREFIX: &'static [u8] = b"MockFundingInfo";
}

const BALANCES: &[u8] = b"BALANCES";
const BONDS: &[u8] = b"BONDS";

impl<T: Chainflip> MockFundingInfo<T> {
	pub fn credit_funds(account_id: &T::AccountId, amount: T::Amount) {
		<Self as MockPalletStorage>::mutate_value(
			BALANCES,
			|storage: &mut Option<BTreeMap<T::AccountId, T::Amount>>| {
				if let Some(balances) = storage.as_mut() {
					balances.entry(account_id.clone()).or_default().saturating_accrue(amount);
				} else {
					let _ = storage.insert(BTreeMap::from_iter([(account_id.clone(), amount)]));
				}
				Ok::<_, Never>(())
			},
		)
		.unwrap();
	}

	pub fn try_debit_funds(account_id: &T::AccountId, amount: T::Amount) -> Option<T::Amount> {
		if amount.is_zero() {
			return Some(amount)
		}
		<Self as MockPalletStorage>::mutate_value(
			BALANCES,
			|storage: &mut Option<BTreeMap<T::AccountId, T::Amount>>| {
				storage.as_mut().and_then(|balances| {
					balances.get_mut(account_id).and_then(|balance| {
						balance.checked_sub(&amount).map(|remainder| {
							*balance = remainder;
							remainder
						})
					})
				})
			},
		)
	}

	pub fn set_balances(balances: impl IntoIterator<Item = (T::AccountId, T::Amount)>) {
		<Self as MockPalletStorage>::mutate_value(BALANCES, |storage| {
			let _ = storage.insert(BTreeMap::from_iter(balances));
			Ok::<_, Never>(())
		})
		.unwrap();
	}
}

impl<T: Chainflip> FundingInfo for MockFundingInfo<T> {
	type AccountId = T::AccountId;
	type Balance = T::Amount;

	fn total_balance_of(account_id: &Self::AccountId) -> Self::Balance {
		<Self as MockPalletStorage>::get_value(BALANCES)
			.and_then(|balances: BTreeMap<Self::AccountId, Self::Balance>| {
				balances.get(account_id).cloned()
			})
			.unwrap_or_default()
	}

	fn total_onchain_funds() -> Self::Balance {
		<Self as MockPalletStorage>::get_value(BALANCES)
			.map(|balances: BTreeMap<Self::AccountId, Self::Balance>| {
				balances.values().cloned().sum()
			})
			.unwrap_or_default()
	}
}

impl<T: Chainflip> AccountInfo for MockFundingInfo<T> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;
	/// Returns the account's total Flip balance.
	fn balance(account_id: &Self::AccountId) -> Self::Amount {
		Self::total_balance_of(account_id)
	}

	/// Returns the bond on the account.
	fn bond(account_id: &Self::AccountId) -> Self::Amount {
		<Self as MockPalletStorage>::get_storage(BONDS, account_id).unwrap_or_default()
	}

	/// Returns the account's liquid funds, net of the bond.
	fn liquid_funds(account_id: &Self::AccountId) -> Self::Amount {
		Self::balance(account_id).saturating_sub(Self::bond(account_id))
	}
}

impl<T: Chainflip> FundAccount for MockFundingInfo<T> {
	type AccountId = u64;
	type Amount = u128;

	#[cfg(feature = "runtime-benchmarks")]
	fn get_bond(account_id: Self::AccountId) -> Self::Amount {
		<Self as MockPalletStorage>::get_value(BONDS)
			.and_then(|balances: BTreeMap<Self::AccountId, Self::Amount>| {
				balances.get(&account_id).cloned()
			})
			.unwrap_or_default()
	}

	fn fund_account(account_id: Self::AccountId, amount: Self::Amount, _source: FundingSource) {
		<Self as MockPalletStorage>::mutate_value(
			BONDS,
			|storage: &mut Option<BTreeMap<Self::AccountId, Self::Amount>>| {
				if let Some(bonds) = storage.as_mut() {
					bonds.entry(account_id).or_default().saturating_accrue(amount);
				} else {
					let _ = storage.insert(BTreeMap::from_iter([(account_id, amount)]));
				}
				Ok::<_, Never>(())
			},
		)
		.unwrap();
	}
}
