#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::common::price_at_tick;
use cf_primitives::Asset;
use cf_traits::{AccountRoleRegistry, LpBalanceApi};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	assert_ok,
	dispatch::UnfilteredDispatchable,
	sp_runtime::traits::One,
	traits::{EnsureOrigin, OnNewAccount},
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};

fn new_lp_account<T: Chainflip>() -> T::AccountId {
	let caller: T::AccountId = whitelisted_caller();
	<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
	T::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
	caller
}

benchmarks! {
	update_buy_interval {
		let call = Call::<T>::update_buy_interval{
			new_buy_interval: BlockNumberFor::<T>::one(),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert_eq!(FlipBuyInterval::<T>::get(), BlockNumberFor::<T>::one());
	}

	update_pool_enabled {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, 0, price_at_tick(0).unwrap());
		let call =  Call::<T>::update_pool_enabled{
			unstable_asset: Asset::Eth,
			enabled: false,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert!(!Pools::<T>::get(Asset::Eth).unwrap().enabled);
	}

	new_pool {
		let call =  Call::<T>::new_pool {
			unstable_asset: Asset::Eth,
			fee_hundredth_pips: 0u32,
			initial_price: price_at_tick(0).unwrap(),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert!(Pools::<T>::get(Asset::Eth).is_some());
	}

	update_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, 0, price_at_tick(0).unwrap()));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Usdc,
			1_000_000,
		));
	}: _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		Asset::Usdc,
		0,
		Some(-100..100),
		RangeOrderSize::AssetAmounts {
			maximum: AssetAmounts {
				base: 1_000_000,
				pair: 1_000_000,
			},
			minimum: AssetAmounts {
				base: 500_000,
				pair: 500_000,
			},
		},
		IncreaseOrDecrease::Increase
	)
	verify {}

	set_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, 0, price_at_tick(0).unwrap()));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Usdc,
			1_000_000,
		));
	}: _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		Asset::Usdc,
		0,
		Some(-100..100),
		RangeOrderSize::AssetAmounts {
			maximum: AssetAmounts {
				base: 1_000_000,
				pair: 1_000_000,
			},
			minimum: AssetAmounts {
				base: 500_000,
				pair: 500_000,
			},
		}
	)
	verify {}

	update_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, 0, price_at_tick(0).unwrap()));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Usdc,
			1_000_000,
		));
	}: _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		Asset::Usdc,
		0,
		Some(100),
		1_000_000,
		IncreaseOrDecrease::Increase
	)
	verify {}

	set_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, 0, price_at_tick(0).unwrap()));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
		assert_ok!(T::LpBalance::try_credit_account(
			&caller,
			Asset::Usdc,
			1_000_000,
		));
	}: _(
		RawOrigin::Signed(caller.clone()),
		Asset::Eth,
		Asset::Usdc,
		0,
		Some(100),
		1_000
	)
	verify {}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
