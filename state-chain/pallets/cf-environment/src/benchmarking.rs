//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;

use frame_benchmarking::whitelisted_caller;
use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	set_system_state {
		let caller: T::AccountId = whitelisted_caller();
		let call = Call::<T>::set_system_state(SystemState::Maintenance);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CurrentSystemState::<T>::get(), SystemState::Maintenance);
	}
}
