use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type ScheduledTxForReject<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;
}

pub struct RenameScheduledTxForReject<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for RenameScheduledTxForReject<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let count = old::ScheduledTxForReject::<T, I>::get().len() as u64;
		Ok(count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::ScheduledTransactionsForRejection::<T, I>::put(
			old::ScheduledTxForReject::<T, I>::take(),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_upgrade_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_upgrade_count =
			crate::ScheduledTransactionsForRejection::<T, I>::get().len() as u64;

		assert_eq!(pre_upgrade_count, post_upgrade_count);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::mock_btc::Test;

	use self::mock_btc::new_test_ext;

	use super::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			old::ScheduledTxForReject::<Test, ()>::put::<
				sp_runtime::Vec<TransactionRejectionDetails<Test, ()>>,
			>(vec![]);
			assert_eq!(old::ScheduledTxForReject::<Test, ()>::get(), vec![]);

			#[cfg(feature = "try-runtime")]
			let state: Vec<u8> = RenameScheduledTxForReject::<Test, ()>::pre_upgrade().unwrap();

			RenameScheduledTxForReject::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			RenameScheduledTxForReject::<Test>::post_upgrade(state).unwrap();

			assert_eq!(ScheduledTransactionsForRejection::<Test>::get(), vec![]);
		});
	}
}
