#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::common::sqrt_price_at_tick;
use cf_primitives::Asset;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	assert_ok,
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnNewAccount},
};
use frame_system::RawOrigin;
use sp_runtime::traits::One;

fn new_lp_account<T: Chainflip>() -> T::AccountId {
	let caller: T::AccountId = whitelisted_caller();
	<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
	T::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
	caller
}

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
		let origin = T::EnsureGovernance::successful_origin();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, 0, sqrt_price_at_tick(0));
		let call =  Call::<T>::update_pool_enabled{
			unstable_asset: Asset::Eth,
			enabled: false,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	} verify {
		assert!(!Pools::<T>::get(Asset::Eth).unwrap().enabled);
	}

	new_pool {
		let call =  Call::<T>::new_pool {
			unstable_asset: Asset::Eth,
			fee_hundredth_pips: 0u32,
			initial_sqrt_price: sqrt_price_at_tick(0),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	} verify {
		assert!(Pools::<T>::get(Asset::Eth).is_some());
	}

	collect_and_mint_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
			-100..100,
			RangeOrderSize::AssetAmounts {
				desired: SideMap::from_array([1_000_000, 1_000_000]),
				minimum: SideMap::from_array([500_000, 500_000]),
			}
	)
	verify {}

	collect_and_burn_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
		assert_ok!(Pallet::<T>::collect_and_mint_range_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			-100..100,
			RangeOrderSize::Liquidity(1_000),
		));
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, -100..100, 1_000)
	verify {}

	collect_and_mint_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, Order::Sell, 100, 1_000_000)
	verify {}

	collect_and_burn_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
		assert_ok!(Pallet::<T>::collect_and_mint_limit_order(RawOrigin::Signed(caller.clone()).into(), Asset::Eth, Order::Sell, 100, 1_000));
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, Order::Sell, 100, 1_000)
	verify {}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
