use crate::*;

use cf_traits::SafeMode;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("UPGRADDDDDDDINNNNNNNNNGGGGGGG");
		TempStorageItem::<T>::put(420);
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, DispatchError> {
		let before = TempStorageItem::<T>::get();
		log::info!("before: {}", before);
		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), DispatchError> {
		let temp: Option<u32> = TempStorageItem::decode(&mut &state[..])
			.map_err(|_| "Failed to decode temp storage item")?;

		log::info!("HALLASDFADFALFKJAS;FKJASD;FKJSFA");

		// should fail
		assert_eq!(temp, Some(100));

		Ok(())
	}
}
