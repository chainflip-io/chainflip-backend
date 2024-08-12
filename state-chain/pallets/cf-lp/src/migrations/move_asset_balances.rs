use crate::*;
use cf_primitives::AccountId;
use cf_traits::HistoricalFeeMigration;
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
}

type AssetBalance<T> = Vec<(<T as frame_system::Config>::AccountId, Asset, u128)>;

#[derive(Encode, Decode)]
pub struct MigrationData<T: Config> {
	pub balances: AssetBalance<T>,
	pub fees: AssetBalance<T>,
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T>
where
	Vec<(AccountId, cf_primitives::Asset, u128)>:
		From<Vec<(<T as frame_system::Config>::AccountId, cf_primitives::Asset, u128)>>,
{
	fn on_runtime_upgrade() -> Weight {
		for (account, asset, amount) in old::FreeBalances::<T>::drain() {
			T::BalanceApi::try_credit_account(&account, asset, amount).expect("Migration failed");
		}
		for (account, asset, amount) in old::HistoricalEarnedFees::<T>::drain() {
			T::MigrationHelper::migrate_historical_fee(account, asset, amount);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let balances: MigrationData<T> = MigrationData {
			balances: old::FreeBalances::<T>::iter().collect::<Vec<_>>(),
			fees: old::HistoricalEarnedFees::<T>::iter().collect::<Vec<_>>(),
		};
		Ok(balances.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let data = MigrationData::<T>::decode(&mut &state[..]).unwrap();
		for (account, asset, amount) in data.balances {
			let new_balance = T::BalanceApi::get_balance(&account, asset);
			assert_eq!(new_balance, amount, "Balance mismatch for {:?}", account);
		}
		for (account, asset, amount) in data.fees {
			let new_fee = T::MigrationHelper::get_fee_amount(account.clone(), asset);
			assert_eq!(new_fee, amount, "Fee mismatch for {:?}", account);
		}
		Ok(())
	}
}
