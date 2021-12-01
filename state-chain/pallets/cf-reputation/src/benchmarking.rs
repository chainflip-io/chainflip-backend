//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet as Reputation;

benchmarks! {
	update_accrual_ratio {
	} : _(RawOrigin::Root, 2, (151 as u32).into())
	// verify {
	// 	assert_eq!(Pallet::<T>::accrual_ratio(), (2, 150).into())
	// }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
