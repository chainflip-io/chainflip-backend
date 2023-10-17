use crate::*;

use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type NextCompatibilityVersion<T: Config> =
		StorageValue<Pallet<T>, Option<SemVer>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		old::NextCompatibilityVersion::<T>::take();

		Weight::zero()
	}
}
