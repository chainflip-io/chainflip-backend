//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	set_active_validator_range {
		let range = (2, 100);
	}: _(RawOrigin::Root, range.into())
	verify {
		assert!(matches!(
			Pallet::<T>::auction_parameters(),
			AuctionParametersV1 { min_size, max_size, .. } if (min_size, max_size) == range
		));
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
