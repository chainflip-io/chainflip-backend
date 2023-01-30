#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_primitives::Asset;
use frame_benchmarking::benchmarks;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use sp_runtime::traits::One;

benchmarks! {
	update_buy_interval {
		let call = Call::<T>::update_buy_interval{
			new_buy_interval: T::BlockNumber::one(),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}

	update_pool_enabled {
		let origin = <T as Config>::EnsureGovernance::successful_origin();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, 0, 0);
		let call =  Call::<T>::update_pool_enabled{
			asset: Asset::Eth,
			enabled: false,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}

	new_pool {
		let call =  Call::<T>::new_pool {
			asset: Asset::Eth,
			fee_100th_bips: 0u32,
			initial_tick_price: 0,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}

	set_liquidity_fee {
		let origin = <T as Config>::EnsureGovernance::successful_origin();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, 0, 0);
		let call =  Call::<T>::set_liquidity_fee {
			asset: Asset::Eth,
			fee_100th_bips: 1u32,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
