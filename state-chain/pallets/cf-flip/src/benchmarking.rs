#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{EnsureOrigin, UnfilteredDispatchable};

benchmarks! {
	set_slashing_rate {
		let slashing_rate: Permill = Permill::one();
		let call = Call::<T>::set_slashing_rate { slashing_rate };
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::slashing_rate(), slashing_rate)
	}

	reap_one_account {
		let caller: T::AccountId = whitelisted_caller();
		Account::<T>::insert(&caller, FlipAccount { balance: T::Balance::from(0u32), bond: T::Balance::from(0u32)});
	}: { BurnFlipAccount::<T>::on_killed_account(&caller); }
	verify {
		assert!(!Account::<T>::contains_key(&caller));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
