use crate::*;

use cf_traits::SafeMode;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		RuntimeSafeMode::<T>::set(SafeMode::CODE_GREEN);

		Weight::zero()
	}
}
