#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::Asset;
use frame_benchmarking::benchmarks;

use frame_support::dispatch::UnfilteredDispatchable;

benchmarks! {
	set_system_state {
		let call = Call::<T>::set_system_state { state: SystemState::Maintenance };
		let origin = T::EnsureGovernance::successful_origin();
	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(CurrentSystemState::<T>::get(), SystemState::Maintenance);
	}
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
		let asset = Asset::Flip;
		let address = [0; 20];
		let call = Call::<T>::update_supported_eth_assets { asset, address };
	}: { call.dispatch_bypass_filter(origin)? }
}
