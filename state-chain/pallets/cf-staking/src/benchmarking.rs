//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	staked {
		const STAKE: T::Balance = T::Balance::from(100 as u32);
		const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
		const TX_HASH: pallet::EthTransactionHash = [211u8; 32];
		let caller: T::AccountId = whitelisted_caller();
		let call = Call::<T>::staked(caller, STAKE, ETH_DUMMY_ADDR, TX_HASH);
		let origin = T::EnsureWitnessed::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
