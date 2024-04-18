#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_slashing_rate() {
		let slashing_rate: Permill = Permill::one();
		let call = Call::<T>::set_slashing_rate { slashing_rate };
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(Pallet::<T>::slashing_rate(), slashing_rate)
	}

	#[benchmark]
	fn reap_one_account() {
		let caller: T::AccountId = whitelisted_caller();
		Account::<T>::insert(
			&caller,
			FlipAccount { balance: T::Balance::from(0u32), bond: T::Balance::from(0u32) },
		);

		#[block]
		{
			BurnFlipAccount::<T>::on_killed_account(&caller);
		}

		assert!(!Account::<T>::contains_key(&caller));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
