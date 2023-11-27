use crate::*;
use frame_support::{migration, pallet_prelude::Weight, traits::OnRuntimeUpgrade};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		migration::move_storage_from_pallet(
			Pallet::<T, I>::storage_metadata().prefix.as_bytes(),
			b"RequestCallbacks",
			b"RequestSuccessCallbacks",
		);
		Weight::zero()
	}
}
