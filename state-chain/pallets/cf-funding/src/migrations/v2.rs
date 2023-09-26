use crate::*;
use frame_support::traits::{OnRuntimeUpgrade, PalletInfoAccess};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::dispatch::DispatchError;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		frame_support::migration::move_prefix(
			&frame_support::storage::storage_prefix(
				Pallet::<T>::name().as_bytes(),
				b"BoundAddress",
			),
			&frame_support::storage::storage_prefix(
				Pallet::<T>::name().as_bytes(),
				b"BoundRedeemAddress",
			),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
