#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::Asset;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, traits::OnNewAccount};
use frame_system::RawOrigin;

benchmarks! {
	request_liquidity_deposit_address {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
		let _ = Pallet::<T>::register_liquidity_refund_address(
			RawOrigin::Signed(caller.clone()).into(),
			EncodedAddress::Eth(Default::default()),
		);
	}: _(RawOrigin::Signed(caller), Asset::Eth)

	withdraw_asset {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
		assert_ok!(Pallet::<T>::try_credit_account(
			&caller,
			Asset::Eth,
			1_000_000,
		));
	}: _(RawOrigin::Signed(caller.clone()), 1_000_000, Asset::Eth, cf_chains::address::EncodedAddress::benchmark_value())
	verify {
		assert_eq!(FreeBalances::<T>::get(&caller, Asset::Eth), Some(0));
	}

	register_lp_account {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
	}: _(RawOrigin::Signed(caller.clone()))
	verify {
		assert_ok!(T::AccountRoleRegistry::ensure_liquidity_provider(RawOrigin::Signed(caller).into()));
	}
	register_liquidity_refund_address {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
	}: _(RawOrigin::Signed(caller.clone()), EncodedAddress::Eth([0x01; 20]))
	verify {
		assert_eq!(LiquidityRefundAddress::<T>::get(caller, ForeignChain::Ethereum), Some(ForeignChainAddress::Eth([0x01; 20].into())));
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
