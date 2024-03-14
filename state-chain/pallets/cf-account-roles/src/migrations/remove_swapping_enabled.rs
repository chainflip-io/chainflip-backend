use crate::Config;
use core::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use frame_support::{ensure, sp_runtime::DispatchError};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use crate::{Config, Pallet};
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias(pallet_name)]
	pub type SwappingEnabled<T: Config> = StorageValue<Pallet<T>, bool, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		old::SwappingEnabled::<T>::kill();

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		ensure!(old::SwappingEnabled::<T>::get(), "SwappingEnabled should be true before upgrade.");
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		ensure!(
			!old::SwappingEnabled::<T>::exists(),
			"SwappingEnabled should be removed during upgrade."
		);
		Ok(())
	}
}
