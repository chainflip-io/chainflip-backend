use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

use frame_support::pallet_prelude::{ValueQuery, Weight};

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type WithheldTransactionFees<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAsset<T, I>,
		TargetChainAmount<T, I>,
		ValueQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		for (asset, fee) in old::WithheldTransactionFees::<T, I>::drain() {
			T::Refunding::with_held_transaction_fees(asset, fee);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		assert_eq!(
			old::WithheldTransactionFees::<T, I>::iter().collect::<Vec<_>>().len(),
			1,
			"Chain can only have one gas asset!"
		);
		let old_fees: u128 =
			old::WithheldTransactionFees::<T, I>::get(<T::TargetChain as Chain>::GAS_ASSET).into();
		Ok(old_fees.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			old::WithheldTransactionFees::<T, I>::iter().collect::<Vec<_>>().len(),
			0,
			"WithheldTransactionFees not empty - migration failed!"
		);
		let old_fees = <u128>::decode(&mut &state[..]).unwrap();
		let migrated_fees =
			T::Refunding::get_withheld_transaction_fees(<T::TargetChain as Chain>::GAS_ASSET);
		assert_eq!(old_fees, migrated_fees, "Migrated fees do not match for asset!");
		Ok(())
	}
}
