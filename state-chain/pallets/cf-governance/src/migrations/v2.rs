use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

use crate::{Config, Members};

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for member in &Members::<T>::get() {
			frame_system::Pallet::<T>::inc_sufficients(member);
		}
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		for member in &Members::<T>::get() {
			assert_eq!(frame_system::Pallet::<T>::sufficients(member), 0);
		}
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		for member in &Members::<T>::get() {
			assert_eq!(frame_system::Pallet::<T>::sufficients(member), 1);
		}
		Ok(())
	}
}
