#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::{AccountRole, Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountRoleRegistry, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OnNewAccount};
use frame_system::RawOrigin;

#[allow(clippy::multiple_bound_locations)]
#[benchmarks(
	where <T::FeePayment as cf_traits::FeePayment>::Amount: From<u128>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn request_liquidity_deposit_address() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::LiquidityProvider,
		)
		.unwrap();
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
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::LiquidityProvider,
		)
		.unwrap();
		T::BalanceApi::credit_account(&caller, Asset::Eth, 1_000_000);

		#[extrinsic_call]
		withdraw_asset(
			RawOrigin::Signed(caller.clone()),
			1_000_000,
			Asset::Eth,
			cf_chains::address::EncodedAddress::benchmark_value(),
		);
	}

	#[benchmark]
	fn register_lp_account() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		frame_system::Pallet::<T>::inc_providers(&caller);

		#[extrinsic_call]
		register_lp_account(RawOrigin::Signed(caller.clone()));

		assert_ok!(T::AccountRoleRegistry::ensure_liquidity_provider(
			RawOrigin::Signed(caller).into()
		));
	}

	#[benchmark]
	fn deregister_lp_account() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::LiquidityProvider,
		)
		.unwrap();

		#[extrinsic_call]
		deregister_lp_account(RawOrigin::Signed(caller.clone()));

		assert!(T::AccountRoleRegistry::ensure_liquidity_provider(
			RawOrigin::Signed(caller).into()
		)
		.is_err());
	}

	#[benchmark]
	fn register_liquidity_refund_address() {
		let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
			AccountRole::LiquidityProvider,
		)
		.unwrap();

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

	#[benchmark]
	fn internal_swap() {
		let lp_id =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::LiquidityProvider)
				.unwrap();

		let caller = RawOrigin::Signed(lp_id.clone());

		assert_ok!(Pallet::<T>::register_liquidity_refund_address(
			caller.clone().into(),
			EncodedAddress::Eth(Default::default()),
		));

		T::BalanceApi::credit_account(&lp_id, Asset::Eth, 1000);

		#[extrinsic_call]
		Pallet::<T>::internal_swap(
			caller,
			1000,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
