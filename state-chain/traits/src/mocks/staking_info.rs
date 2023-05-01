use crate::{Chainflip, StakingInfo};
use frame_support::Never;
use sp_runtime::{traits::CheckedSub, Saturating};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use super::{MockPallet, MockPalletStorage};

pub struct MockStakingInfo<T>(PhantomData<T>);

impl<T> MockPallet for MockStakingInfo<T> {
	const PREFIX: &'static [u8] = b"MockStakingInfo";
}

const STAKES: &[u8] = b"Stakes";

impl<T: Chainflip> MockStakingInfo<T> {
	pub fn credit_stake(account_id: &T::AccountId, amount: T::Amount) {
		<Self as MockPalletStorage>::mutate_value(
			STAKES,
			|storage: &mut Option<BTreeMap<T::AccountId, T::Amount>>| {
				if let Some(stakes) = storage.as_mut() {
					stakes.entry(account_id.clone()).or_default().saturating_accrue(amount);
				} else {
					let _ = storage.insert(BTreeMap::from_iter([(account_id.clone(), amount)]));
				}
				Ok::<_, Never>(())
			},
		)
		.unwrap();
	}

	pub fn try_debit_stake(account_id: &T::AccountId, amount: T::Amount) -> Option<T::Amount> {
		<Self as MockPalletStorage>::mutate_value(
			STAKES,
			|storage: &mut Option<BTreeMap<T::AccountId, T::Amount>>| {
				storage.as_mut().and_then(|stakes| {
					stakes.get_mut(account_id).and_then(|stake| {
						stake.checked_sub(&amount).map(|remainder| {
							*stake = remainder;
							remainder
						})
					})
				})
			},
		)
	}

	pub fn set_stakes(stakes: impl IntoIterator<Item = (T::AccountId, T::Amount)>) {
		<Self as MockPalletStorage>::mutate_value(STAKES, |storage| {
			let _ = storage.insert(BTreeMap::from_iter(stakes));
			Ok::<_, Never>(())
		})
		.unwrap();
	}
}

impl<T: Chainflip> StakingInfo for MockStakingInfo<T> {
	type AccountId = T::AccountId;
	type Balance = T::Amount;

	fn total_stake_of(account_id: &Self::AccountId) -> Self::Balance {
		<Self as MockPalletStorage>::get_value(STAKES)
			.and_then(|stakes: BTreeMap<Self::AccountId, Self::Balance>| {
				stakes.get(account_id).cloned()
			})
			.unwrap_or_default()
	}

	fn total_onchain_stake() -> Self::Balance {
		<Self as MockPalletStorage>::get_value(STAKES)
			.map(|stakes: BTreeMap<Self::AccountId, Self::Balance>| stakes.values().cloned().sum())
			.unwrap_or_default()
	}
}
