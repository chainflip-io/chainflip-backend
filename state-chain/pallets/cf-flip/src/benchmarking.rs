#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	set_slashing_rate {
		let balance: T::Balance = T::Balance::from(100 as u32);
		let call = Call::<T>::set_slashing_rate(balance);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::slashing_rate(), balance.into())
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
