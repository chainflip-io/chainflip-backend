//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Auction;

benchmarks! {
	// witness_eth_signature_success {
	// 	let caller = whitelisted_caller();
	// 	let id: u64 = 1;
	// 	let signature: u128 = 12345;
	// } : _(RawOrigin::Signed(caller), id, signature)
	witness_eth_signature_success {} : {}
	// TODO: implement benchmark
	witness_eth_signature_failed {
		let caller = whitelisted_caller();
	} : _(RawOrigin::Signed(caller), 1, vec![])
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
