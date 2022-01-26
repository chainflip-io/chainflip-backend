//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	update_eth_block_safety_margin {
		let call = Call::<T>::update_eth_block_safety_margin(1);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::cfe_settings().eth_block_safety_margin, 1);
	}
	update_max_retry_attempts {
		let call = Call::<T>::update_max_retry_attempts(2);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::cfe_settings().max_extrinsic_retry_attempts, 2);
	}
	update_max_stage_duration {
		let call = Call::<T>::update_max_stage_duration(3);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::cfe_settings().max_ceremony_stage_duration, 3);
	}
	update_pending_sign_duration {
		let call = Call::<T>::update_pending_sign_duration(4);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::cfe_settings().pending_sign_duration, 4);
	}
}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
