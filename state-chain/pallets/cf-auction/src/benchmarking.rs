//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	set_auction_parameters {
		let origin = T::EnsureGovernance::successful_origin();
		let params = SetSizeParameters {
			min_size: 3,
			max_size: 150,
			max_expansion: 15,
		};
		let call = Call::<T>::set_auction_parameters{parameters: params};
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(
			Pallet::<T>::auction_parameters(),
			params
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
