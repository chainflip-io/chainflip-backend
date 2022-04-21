//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;

use frame_benchmarking::whitelisted_caller;
use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	set_network_state {
		let caller: T::AccountId = whitelisted_caller();
		let call = Call::<T>::set_network_state(NetworkState::Paused);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CurrentNetworkState::<T>::get(), NetworkState::Paused);
	}
}
