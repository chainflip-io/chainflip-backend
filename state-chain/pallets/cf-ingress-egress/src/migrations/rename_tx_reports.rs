use core::marker::PhantomData;

use frame_support::{
	traits::{OnRuntimeUpgrade, PalletInfoAccess},
	weights::Weight,
};

use crate::{Config, Pallet};

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let _ = frame_support::storage::unhashed::clear_prefix(
			frame_support::storage::storage_prefix(
				Pallet::<T, I>::name().as_bytes(),
				b"TaintedTransactions",
			)
			.as_slice(),
			None,
			None,
		);

		Default::default()
	}
}
