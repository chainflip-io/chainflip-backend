use cf_chains::Chain;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

use crate::ChainInitialized;
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: crate::Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		assert_ne!(<T::Chain as Chain>::NAME, "Arbitrum", "Arbitrum should not exist yet.");

		ChainInitialized::<T, I>::put(true);
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		assert!(!ChainInitialized::<T, I>::exists());
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	#[allow(clippy::bool_assert_comparison)]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert!(ChainInitialized::<T, I>::exists());
		Ok(())
	}
}
