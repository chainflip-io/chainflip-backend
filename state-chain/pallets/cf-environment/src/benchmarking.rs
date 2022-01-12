//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use sp_std::time::Duration;

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	update_eth_block_safety_margin {
		let call = Call::<T>::update_eth_block_safety_margin(4);
		let origin = T::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CFESettings::<T>::get().eth_block_safety_margin, 4);
	}
	update_pending_sign_duration {
		let duration = Duration::from_secs(500);
		let call = Call::<T>::update_pending_sign_duration(duration.clone());
		let origin = T::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CFESettings::<T>::get().pending_sign_duration, duration);
	}
	update_max_stage_duration {
		let duration = Duration::from_secs(500);
		let call = Call::<T>::update_max_stage_duration(duration.clone());
		let origin = T::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CFESettings::<T>::get().max_stage_duration, duration);
	}
	update_max_retry_attempts {
		let call = Call::<T>::update_max_retry_attempts(4);
		let origin = T::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CFESettings::<T>::get().max_retry_attempts, 4);
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
