use crate::{Chainflip, FundingInfo};
use frame_support::Never;
use sp_runtime::{
	traits::{CheckedSub, Zero},
	Saturating,
};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use super::{MockPallet, MockPalletStorage};

pub struct MockFundingInfo<T>(PhantomData<T>);

impl<T> MockPallet for MockFundingInfo<T> {
	const PREFIX: &'static [u8] = b"MockFundingInfo";
}

const BALANCES: &[u8] = b"BALANCES";

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
