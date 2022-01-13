//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	update_cfe_value {
		let call = Call::<T>::update_cfe_value(cfe::CFESettingKeys::EthBlockSafetyMargin, 4);
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(Pallet::<T>::cfe_settings().eth_block_safety_margin, 4);
	}
}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
