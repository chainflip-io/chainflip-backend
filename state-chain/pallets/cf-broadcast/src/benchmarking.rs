//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Auction;

benchmarks! {
	// TODO: implement benchmark
	on_initialize {} : {}
	// TODO: implement benchmark
	start_broadcast {} : {}
	// TODO: implement benchmark
	transaction_ready_for_transmission {} : {}
	// TODO: implement benchmark
	transmission_success {} : {}
	// TODO: implement benchmark
	transmission_failure {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
