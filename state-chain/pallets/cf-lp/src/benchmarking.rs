#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::{AccountRole, Asset};
use cf_traits::{AccountRoleRegistry, LiquidityPoolApi};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::assert_ok;
use frame_system::RawOrigin;

benchmarks! {
	request_deposit_address {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
	}: _(RawOrigin::Signed(caller), Asset::Eth)


	withdraw_asset {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		assert_ok!(Pallet::<T>::provision_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
	}: _(RawOrigin::Signed(caller), 1_000_000, Asset::Eth, ForeignChainAddress::benchmark_value())

	register_lp_account {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::None);
	}:  _(RawOrigin::Signed(caller))

	update_position {
		let caller: T::AccountId = whitelisted_caller();
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::LiquidityProvider);
		T::LiquidityPoolApi::new_pool(Asset::Eth);
		assert_ok!(Pallet::<T>::provision_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
		assert_ok!(Pallet::<T>::provision_account(
			&caller,
			Asset::Usdc,
			1_000_000,
		));
		let range = AmmRange::new(-100, 100);
	}: _(RawOrigin::Signed(caller), Asset::Eth, range, 1_000)

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
