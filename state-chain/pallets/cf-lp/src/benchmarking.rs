#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::Asset;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable, traits::OnNewAccount};
use frame_system::RawOrigin;

benchmarks! {
	request_liquidity_deposit_address {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
		let _ = Pallet::<T>::register_emergency_withdrawal_address(
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

	on_initialize {
		let a in 1..100;
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		let _ = Pallet::<T>::register_lp_account(RawOrigin::Signed(caller.clone()).into());
		let _ = Pallet::<T>::register_emergency_withdrawal_address(
			RawOrigin::Signed(caller.clone()).into(),
			EncodedAddress::Eth(Default::default()),
		);
		for i in 0..a {
			assert_ok!(Pallet::<T>::request_liquidity_deposit_address(RawOrigin::Signed(caller.clone()).into(), Asset::Eth));
		}
		let expiry = LpTTL::<T>::get() + frame_system::Pallet::<T>::current_block_number();
		assert!(!LiquidityChannelExpiries::<T>::get(expiry).is_empty());
	}: {
		Pallet::<T>::on_initialize(expiry);
	} verify {
		assert!(LiquidityChannelExpiries::<T>::get(expiry).is_empty());
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

	register_emergency_withdrawal_address {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
	}: _(RawOrigin::Signed(caller.clone()), EncodedAddress::Eth([0x01; 20]))
	verify {
		assert_eq!(EmergencyWithdrawalAddress::<T>::get(caller, ForeignChain::Ethereum), Some(ForeignChainAddress::Eth([0x01; 20].into())));
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
