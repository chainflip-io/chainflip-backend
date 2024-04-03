use crate::*;

use cf_chains::Bitcoin;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

const BTC_FEE_MULTIPLIER: FixedU128 = FixedU128::from_rational(3, 2);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if T::TargetChain::NAME == "Bitcoin" {
			FeeMultiplier::<T, I>::set(BTC_FEE_MULTIPLIER);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use frame_support::sp_runtime::traits::One;
		if T::TargetChain::NAME == <Bitcoin as Chain>::NAME {
			assert_eq!(FeeMultiplier::<T, I>::get(), BTC_FEE_MULTIPLIER);
		} else {
			assert_eq!(FeeMultiplier::<T, I>::get(), FixedU128::one());
		}

		Ok(())
	}
}
