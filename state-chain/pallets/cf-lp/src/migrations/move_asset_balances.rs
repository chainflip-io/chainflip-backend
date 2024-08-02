use crate::*;
use cf_primitives::{AccountId, BalancesInfo};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::vec::Vec;

mod old {

	use super::*;

	#[frame_support::storage_alias]
	pub type FreeBalances<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Identity,
		Asset,
		AssetAmount,
	>;

	#[frame_support::storage_alias]
	pub type HistoricalEarnedFees<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		Twox64Concat,
		Asset,
		AssetAmount,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub type CollectedRejectedFunds<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, Asset, AssetAmount, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T>
where
	Vec<(AccountId, cf_primitives::Asset, u128)>:
		From<Vec<(<T as frame_system::Config>::AccountId, cf_primitives::Asset, u128)>>,
{
	fn on_runtime_upgrade() -> Weight {
		for (account, asset, amount) in old::FreeBalances::<T>::drain() {
			let _ = T::BalanceApi::try_credit_account(&account, asset, amount);
		}
		for (account, asset, amount) in old::HistoricalEarnedFees::<T>::drain() {
			T::BalanceApi::record_fees(&account, amount, asset);
		}
		for (asset, amount) in old::CollectedRejectedFunds::<T>::drain() {
			T::BalanceApi::collected_rejected_funds(asset, amount);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let balances = BalancesInfo {
			rejected_funds: old::CollectedRejectedFunds::<T>::iter().collect::<Vec<_>>(),
			balances: old::FreeBalances::<T>::iter().collect::<Vec<_>>().into(),
			fees: old::HistoricalEarnedFees::<T>::iter().collect::<Vec<_>>().into(),
		};
		Ok(balances.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_balances = BalancesInfo::decode(&mut &state[..]).unwrap();
		let new_balances = T::BalanceApi::get_balances_info();
		assert!(
			old_balances.encode() == new_balances.encode(),
			"Balances do not match after migration"
		);
		Ok(())
	}
}
