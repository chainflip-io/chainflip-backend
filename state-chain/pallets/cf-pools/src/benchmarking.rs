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
	} verify {
		assert_eq!(FlipBuyInterval::<T>::get(), T::BlockNumber::one());
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
	} verify {
		assert!(!Pools::<T>::get(Asset::Eth).unwrap().pool_enabled());
	}

	new_pool {
		let call =  Call::<T>::new_pool {
			asset: Asset::Eth,
			fee_100th_bips: 0u32,
			initial_tick_price: 0,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	} verify {
		assert!(Pools::<T>::get(Asset::Eth).is_some());
	}

	set_liquidity_fee {
		let origin = <T as Config>::EnsureGovernance::successful_origin();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, 0, 0);
		let call =  Call::<T>::set_liquidity_fee {
			asset: Asset::Eth,
			fee_100th_bips: 123u32,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	} verify {
		assert_eq!(Pools::<T>::get(Asset::Eth).unwrap().get_liquidity_fees(), 123u32);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
