//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks_instance_pallet, impl_benchmark_test_suite};

#[allow(unused)]
use crate::Pallet;

benchmarks_instance_pallet! {
	signature_success {} : {}
	signature_failed {} : {}
	on_initialize {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
