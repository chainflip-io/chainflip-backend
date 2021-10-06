//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet as Governance;

benchmarks! {
	new_membership_set {
		let EVE: T::AccountId = account("eve", 0, 0);
		let PETER: T::AccountId = account("peter", 0, 0);
		let MAX: T::AccountId = account("max", 0, 0);
		let ALICE: T::AccountId = account("alice", 0, 0);
		let caller: T::AccountId = whitelisted_caller();
		let members = vec![EVE, PETER, MAX];
		<Members<T>>::put(vec![caller.clone()]);
		//Pallet::<T>::auction_size_range()
	}: _(RawOrigin::Signed(caller.clone()), members)
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
