//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Reputation;

benchmarks! {
	update_accrual_ratio {
		let caller: T::AccountId = whitelisted_caller();
	} : _(RawOrigin::Signed(caller), 2, (150 as u32).into())
	// verify {
	// 	assert_eq!(Pallet::<T>::accrual_ratio(), (2, 150).into())
	// }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
