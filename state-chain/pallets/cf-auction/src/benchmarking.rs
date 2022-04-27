//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	set_auction_parameters {
		let params = DynamicSetSizeParameters {
			min_size: 3,
			max_size: 150,
			max_contraction: 10,
			max_expansion: 15,
		};
	}: _(RawOrigin::Root, params)
	verify {
		assert_eq!(
			Pallet::<T>::auction_parameters(),
			params
		);
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
