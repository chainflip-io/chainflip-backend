#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::{AccountRole, Asset};
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable};
use frame_system::RawOrigin;

benchmarks! {
	request_deposit_address {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
	}: _(RawOrigin::Signed(caller), Asset::Eth)
	withdraw_asset {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
	}: _(RawOrigin::Signed(caller.clone()), 1_000_000, Asset::Eth, ForeignChainAddress::benchmark_value())
	verify {
		assert_eq!(FreeBalances::<T>::get(&caller, Asset::Eth), Some(0));
	}
	register_lp_account {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::None);
	}:  _(RawOrigin::Signed(caller.clone()))
	verify {
		assert_eq!(T::AccountRoleRegistry::get_account_role(caller), AccountRole::LiquidityProvider);
	}

	on_initialize {
		let a in 1..100;
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		for i in 1..a {
			assert_ok!(Pallet::<T>::request_deposit_address(RawOrigin::Signed(caller.clone()).into(), Asset::Eth));
		}
	}: {
		Pallet::<T>::on_initialize(T::BlockNumber::from(1u32));
	}

	set_lp_ttl {
		let ttl = T::BlockNumber::from(1_000u32);
		let call = Call::<T>::set_lp_ttl {
			ttl,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	} verify {
		assert_eq!(crate::LpTTL::<T>::get(), ttl);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
