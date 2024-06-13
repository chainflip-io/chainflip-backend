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
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		// assert_eq!(
		// 	old::WithheldTransactionFees::<T, I>::decoded_len(),
		// 	None,
		// 	"WithheldTransactionFees not empty - migration failed!"
		// );
		Ok(())
	}
}
