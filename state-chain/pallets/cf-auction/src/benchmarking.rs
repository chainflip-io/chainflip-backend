//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	set_current_authority_set_size_range {
		let range = (2, 100);
	}: _(RawOrigin::Root, range.into())
	verify {
		assert_eq!(Pallet::<T>::current_authority_set_size_range(), range.into())
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
