//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]
use super::*;

use cf_traits::Slashing;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_support::traits::EnsureOrigin;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::FlipSlasher;
#[allow(unused)]
use crate::Pallet;

benchmarks! {
	set_slashing_rate {
		let balance: T::Balance = T::Balance::from(100 as u32);
		let call = Call::<T>::set_slashing_rate(balance);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	// slash {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let balance: T::Balance = T::Balance::from(100 as u32);
	// 	const BLOCKS_PER_DAY: u128 = 60;
	// 	<SlashingRate::<T>>::set(balance);
	// }: {
	// 	// TODO: does not compile - function or associated item not found in `FlipSlasher<T>
	// 	FlipSlasher::<T>::slash(&caller, 60 as u128);
	// }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
