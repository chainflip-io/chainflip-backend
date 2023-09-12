#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;

use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {

	update_safe_mode {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::update_safe_mode { update: SafeModeUpdate::CodeRed };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(RuntimeSafeMode::<T>::get(), SafeMode::CODE_RED);
	}

	set_next_compatible_version {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let version = SemVer { major: 1u8, minor: 1u8, patch: 1u8 };
		let call = Call::<T>::set_next_compatible_version { version: Some(version) };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(NextCompatibleVersion::<T>::get(), Some(version));
	}
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
