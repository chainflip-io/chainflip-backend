use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration<T>(PhantomData<T>);

impl<T> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		unimplemented!()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		unimplemented!()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		unimplemented!()
	}
}
