//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]
use super::*;

use frame_system::RawOrigin;
use frame_benchmarking::{benchmarks, whitelisted_caller, impl_benchmark_test_suite};
use sp_std::{vec, vec::Vec, boxed::Box};

#[allow(unused)]
use crate::Pallet as Validator;

benchmarks! {
	set_blocks_for_epoch {
		let s in 2 .. 100;
	}: _(RawOrigin::Root, s.into())
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), s.into())
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(),
	crate::mock::Test,
);