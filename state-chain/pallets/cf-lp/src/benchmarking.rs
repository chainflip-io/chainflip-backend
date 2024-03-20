#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::{Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountRoleRegistry, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OnNewAccount};
use frame_system::RawOrigin;

#[benchmarks(
	where <T::FeePayment as cf_traits::FeePayment>::Amount: From<u128>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn request_liquidity_deposit_address() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		<T as Chainflip>::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
		assert_ok!(Pallet::<T>::register_liquidity_refund_address(
			RawOrigin::Signed(caller.clone()).into(),
			EncodedAddress::Eth(Default::default()),
		));
		// A non-zero balance is required to pay for the channel opening fee.
		T::FeePayment::mint_to_account(&caller, (5 * FLIPPERINOS_PER_FLIP).into());

		#[extrinsic_call]
		request_liquidity_deposit_address(RawOrigin::Signed(caller), Asset::Eth, 0);
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
