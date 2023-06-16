#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;

use cf_chains::dot::RuntimeVersion;
use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	set_cfe_settings {
		let cfe_settings = cfe::CfeSettings {
			eth_priority_fee_percentile: 50,
			eth_block_safety_margin: 4,
			max_ceremony_stage_duration: 1000,
		};
		let call = Call::<T>::set_cfe_settings { cfe_settings };
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CfeSettings::<T>::get(), cfe_settings);
	}
	update_supported_eth_assets {
		let origin = T::EnsureGovernance::successful_origin();
		let asset = EthAsset::Flip;
		let address = Default::default();
		let call = Call::<T>::update_supported_eth_assets { asset, address };
	}: { call.dispatch_bypass_filter(origin)? }
	update_polkadot_runtime_version {
		let origin = T::EnsureWitnessed::successful_origin();
		const POLKADOT_TEST_RUNTIME_VERSION: RuntimeVersion = RuntimeVersion { spec_version: 9360, transaction_version: 19 };
		let runtime_version = RuntimeVersion { spec_version: POLKADOT_TEST_RUNTIME_VERSION.spec_version + 1, transaction_version: 1 };
		let call = Call::<T>::update_polkadot_runtime_version { runtime_version };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(PolkadotRuntimeVersion::<T>::get(), runtime_version);
	}
	update_safe_mode {
		let origin = T::EnsureGovernance::successful_origin();
		let call = Call::<T>::update_safe_mode { update: SafeModeUpdate::CodeRed };
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(RuntimeSafeMode::<T>::get(), SafeMode::CODE_RED);
	}
}
