#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::common::sqrt_price_at_tick;
use cf_primitives::{AccountRole, Asset};
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;
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
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::new_pool(<T as Config>::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, -100..100, 1_000_000)
	verify {}

	collect_and_burn_range_order {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::new_pool(<T as Config>::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
		assert_ok!(Pallet::<T>::collect_and_mint_range_order(RawOrigin::Signed(caller.clone()).into(), Asset::Eth, -100..100, 1_000));
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, -100..100, 1_000)
	verify {}

	collect_and_mint_limit_order {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::new_pool(<T as Config>::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, Side::Zero, 100, 1_000_000)
	verify {}

	collect_and_burn_limit_order {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::new_pool(<T as Config>::EnsureGovernance::successful_origin(), Asset::Eth, 0, sqrt_price_at_tick(0)));
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
		assert_ok!(Pallet::<T>::collect_and_mint_limit_order(RawOrigin::Signed(caller.clone()).into(), Asset::Eth, Side::Zero, 100, 1_000));
	}: _(RawOrigin::Signed(caller.clone()), Asset::Eth, Side::Zero, 100, 1_000)
	verify {}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
