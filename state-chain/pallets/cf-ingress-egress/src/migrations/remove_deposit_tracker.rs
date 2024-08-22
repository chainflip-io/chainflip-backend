use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

use frame_support::{
	pallet_prelude::{ValueQuery, Weight},
	DefaultNoBound,
};

mod old {
	use super::*;
	#[derive(
		CloneNoBound,
		DefaultNoBound,
		RuntimeDebug,
		PartialEq,
		Eq,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositTracker<T: Config<I>, I: 'static> {
		pub unfetched: TargetChainAmount<T, I>,
		pub fetched: TargetChainAmount<T, I>,
	}

	#[frame_support::storage_alias]
	pub type DepositBalances<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAsset<T, I>,
		DepositTracker<T, I>,
		ValueQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let _ = old::DepositBalances::<T, I>::clear(u32::MAX, None);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(old::DepositBalances::<T, I>::iter_keys().count(), 0);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock_eth::*;

		new_test_ext().execute_with(|| {
			let asset = cf_chains::assets::eth::Asset::Eth;
			old::DepositBalances::<Test, _>::set(
				asset,
				old::DepositTracker { unfetched: 1_000_000, fetched: 2_000_000 },
			);

			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(vec![]).unwrap();

			// Storage is cleared
			assert_eq!(old::DepositBalances::<Test, _>::get(asset), Default::default());
		});
	}
}
