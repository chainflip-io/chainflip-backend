use crate::*;
use cf_chains::Bitcoin;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

const BTC_FEE_MULTIPLIER: FixedU128 = FixedU128::from_rational(3, 2);

// No need to migrate Eth or Dot, as the fee multiplier is not used there
impl<T: Config<Instance1>> OnRuntimeUpgrade for Migration<T, Instance1> {
	fn on_runtime_upgrade() -> Weight {
		FeeMultiplier::<T, Instance1>::set(FixedU128::from_u32(1));

		Weight::zero()
	}
}

impl<T: Config<Instance2>> OnRuntimeUpgrade for Migration<T, Instance2> {
	fn on_runtime_upgrade() -> Weight {
		FeeMultiplier::<T, Instance2>::set(FixedU128::from_u32(1));

		Weight::zero()
	}
}

impl<T: Config<Instance3, TargetChain = Bitcoin>> OnRuntimeUpgrade for Migration<T, Instance3> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		FeeMultiplier::<T, Instance3>::set(BTC_FEE_MULTIPLIER);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(FeeMultiplier::<T, Instance3>::get(), BTC_FEE_MULTIPLIER);

		Ok(())
	}
}
