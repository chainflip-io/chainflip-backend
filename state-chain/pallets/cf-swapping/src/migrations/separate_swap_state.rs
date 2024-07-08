use frame_support::traits::OnRuntimeUpgrade;

use crate::*;
use core::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	#[frame_support::storage_alias]
	pub type FirstUnprocessedBlock<T: Config> =
		StorageValue<Pallet<T>, BlockNumberFor<T>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// Note: we don't migrate items from SwapQueue because we will
		// ensure that it is empty during the upgrade.

		// No longer needed:
		old::FirstUnprocessedBlock::<T>::kill();

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
