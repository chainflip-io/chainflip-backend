#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::Asset;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OnNewAccount};
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn request_liquidity_deposit_address() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
		assert_ok!(Pallet::<T>::register_liquidity_refund_address(
			RawOrigin::Signed(caller.clone()).into(),
			EncodedAddress::Eth(Default::default()),
		));

		#[extrinsic_call]
		request_liquidity_deposit_address(RawOrigin::Signed(caller), Asset::Eth);
	}

	#[benchmark]
	fn withdraw_asset() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller));
		assert_ok!(Pallet::<T>::try_credit_account(&caller, Asset::Eth, 1_000_000,));

		#[extrinsic_call]
		withdraw_asset(
			RawOrigin::Signed(caller.clone()),
			1_000_000,
			Asset::Eth,
			cf_chains::address::EncodedAddress::benchmark_value(),
		);

		assert_eq!(FreeBalances::<T>::get(&caller, Asset::Eth), Some(0));
	}

	#[benchmark]
	fn register_lp_account() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_lp_account(RawOrigin::Signed(caller.clone()));

		assert_ok!(T::AccountRoleRegistry::ensure_liquidity_provider(
			RawOrigin::Signed(caller).into()
		));
	}

	#[benchmark]
	fn register_liquidity_refund_address() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller));

		#[extrinsic_call]
		register_liquidity_refund_address(
			RawOrigin::Signed(caller.clone()),
			EncodedAddress::Eth([0x01; 20]),
		);

		assert_eq!(
			LiquidityRefundAddress::<T>::get(caller, ForeignChain::Ethereum),
			Some(ForeignChainAddress::Eth([0x01; 20].into()))
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
