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
	witness_eth_signature_success {} : {}
	// TODO: implement benchmark
	witness_eth_signature_failed {} : {}
	// TODO: implement benchmark
	witness_eth_transmission_success {} : {}
	// TODO: implement benchmark
	witness_eth_transmission_failure {} : {}
	// TODO: implement benchmark
	witness_staked {} : {}
	// TODO: implement benchmark
	witness_claimed {} : {}
	// TODO: implement benchmark
	witness_keygen_success {} : {}
	// TODO: implement benchmark
	witness_keygen_failure {} : {}
	// TODO: implement benchmark
	witness_vault_key_rotated {} : {}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
