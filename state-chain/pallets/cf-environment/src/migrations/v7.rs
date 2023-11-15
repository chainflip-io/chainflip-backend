use crate::*;

use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("Upgrading to version 7");
		TempStorageItem::<T>::put(420);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, DispatchError> {
		let before = TempStorageItem::<T>::get();

		log::info!("before: {:?}", before);
		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), DispatchError> {
		log::info!("post upgrade for v7 running");
		let before = <Option<u32>>::decode(&mut &state[..])
			.map_err(|_| "Failed to decode temp storage item")?;

		log::info!("post upgrade for v7 running");

		// should fail
		assert_eq!(before, Some(100));

		Ok(())
	}
}
