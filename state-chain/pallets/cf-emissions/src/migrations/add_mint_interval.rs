use crate::*;

use frame_support::weights::RuntimeDbWeight;
use sp_std::marker::PhantomData;

// The value for the MintInterval
// runtime constant in pallet version V0
const MINT_INTERVAL_V0: u32 = 100;

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		MintInterval::<T>::put(T::BlockNumber::from(MINT_INTERVAL_V0));
		RuntimeDbWeight::default().reads_writes(0, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		assert_eq!(T::BlockNumber::from(100 as u32), MintInterval::<T>::get());
		log::info!(
			target: "runtime::cf_emissions",
			"migration: Emissions storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
