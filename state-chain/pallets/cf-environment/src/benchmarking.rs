#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;

use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	update_supported_eth_assets {
		let origin = T::EnsureGovernance::successful_origin();
		let asset = EthAsset::Flip;
		let address = Default::default();
		let call = Call::<T>::update_supported_eth_assets { asset, address };
	}: { call.dispatch_bypass_filter(origin)? }

	update_safe_mode {
		let origin = T::EnsureGovernance::successful_origin();
		let call = Call::<T>::update_safe_mode { update: SafeModeUpdate::CodeRed };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(RuntimeSafeMode::<T>::get(), SafeMode::CODE_RED);
	}

	set_next_compatibility_version {
		let origin = T::EnsureGovernance::successful_origin();
		let version = SemVer { major: 1u8, minor: 1u8, patch: 1u8 };
		let call = Call::<T>::set_next_compatibility_version { version: Some(version) };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(NextCompatibilityVersion::<T>::get(), Some(version));
	}
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
