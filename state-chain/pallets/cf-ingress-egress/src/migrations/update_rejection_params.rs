use crate::Config;
use core::marker::PhantomData;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// In order to correctly migrate, we would need to find invent the deposit address.
		// For simplicity, we will just clear the storage.
		crate::ScheduledTxForReject::<T, I>::kill();
		Default::default()
	}
}
