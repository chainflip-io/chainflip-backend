//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{
	benchmarks, benchmarks_instance_pallet, impl_benchmark_test_suite, whitelisted_caller,
};

use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet as Auction;

benchmarks_instance_pallet! {
	signature_success {} : {}
	signature_failed {} : {}
	on_initialize {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
