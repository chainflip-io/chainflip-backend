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
		let old: Vec<TransactionRejectionDetails<T, I>> = old::ScheduledTxForReject::<T, I>::take();
		old.iter().for_each(|elem| {
			crate::ScheduledTransactionsForRejection::<T, I>::append(elem);
		});
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
