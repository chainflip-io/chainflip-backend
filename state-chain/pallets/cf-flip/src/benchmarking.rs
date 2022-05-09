#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_benchmarking::benchmarks;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

benchmarks! {
	set_slashing_rate {
		let slashing_rate: T::Balance = T::Balance::from(100u32);
		let call = Call::<T>::set_slashing_rate { slashing_rate };
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::slashing_rate(), slashing_rate.into())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
