#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::common::price_at_tick;
use cf_chains::ForeignChainAddress;
use cf_primitives::Asset;
use cf_traits::{AccountRoleRegistry, LpBalanceApi};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	sp_runtime::traits::One,
	traits::{EnsureOrigin, OnNewAccount, UnfilteredDispatchable},
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};

fn new_lp_account<T: Chainflip + Config>() -> T::AccountId {
	let caller: T::AccountId = whitelisted_caller();
	<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
	T::AccountRoleRegistry::register_as_liquidity_provider(&caller).unwrap();
	for address in [
		ForeignChainAddress::Eth(Default::default()),
		ForeignChainAddress::Dot(Default::default()),
		ForeignChainAddress::Btc(cf_chains::btc::ScriptPubkey::P2PKH(Default::default())),
	] {
		T::LpBalance::register_liquidity_refund_address(&caller, address);
	}
	caller
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_buy_interval() {
		let call = Call::<T>::update_buy_interval { new_buy_interval: BlockNumberFor::<T>::one() };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(FlipBuyInterval::<T>::get(), BlockNumberFor::<T>::one());
	}

	#[benchmark]
	fn new_pool() {
		let call = Call::<T>::new_pool {
			base_asset: Asset::Eth,
			quote_asset: Asset::Usdc,
			fee_hundredth_pips: 0u32,
			initial_price: price_at_tick(0).unwrap(),
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert!(Pools::<T>::get(AssetPair::new(Asset::Eth, STABLE_ASSET).unwrap()).is_some());
	}

	#[benchmark]
	fn update_range_order() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));

		#[extrinsic_call]
		update_range_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			0,
			Some(-100..100),
			IncreaseOrDecrease::Increase(RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
				minimum: AssetAmounts { base: 500_000, quote: 500_000 },
			}),
		);
	}

	#[benchmark]
	fn set_range_order() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));

		#[extrinsic_call]
		set_range_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			0,
			Some(-100..100),
			RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
				minimum: AssetAmounts { base: 500_000, quote: 500_000 },
			},
		);
	}

	#[benchmark]
	fn update_limit_order() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));

		#[extrinsic_call]
		update_limit_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			Order::Sell,
			0,
			Some(100),
			IncreaseOrDecrease::Increase(1_000_000),
		);
	}

	#[benchmark]
	fn set_limit_order() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));

		#[extrinsic_call]
		set_limit_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			Order::Sell,
			0,
			Some(100),
			1_000,
		);
	}

	#[benchmark]
	fn set_pool_fees() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			Asset::Usdc,
			Order::Buy,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			Asset::Usdc,
			Order::Sell,
			1,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 1_000));
		let fee = 1_000;
		let call = Call::<T>::set_pool_fees {
			base_asset: Asset::Eth,
			quote_asset: Asset::Usdc,
			fee_hundredth_pips: fee,
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(
			Pallet::<T>::pool_info(Asset::Eth, STABLE_ASSET),
			Ok(PoolInfo {
				limit_order_fee_hundredth_pips: fee,
				range_order_fee_hundredth_pips: fee,
			})
		);
	}

	#[benchmark]
	fn schedule_limit_order_update() {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Eth, 1_000_000,));
		assert_ok!(T::LpBalance::try_credit_account(&caller, Asset::Usdc, 1_000_000,));
		#[extrinsic_call]
		schedule_limit_order_update(
			RawOrigin::Signed(caller.clone()),
			Box::new(Call::<T>::set_limit_order {
				base_asset: Asset::Eth,
				quote_asset: Asset::Usdc,
				side: Order::Sell,
				id: 0,
				option_tick: Some(0),
				sell_amount: 100,
			}),
			BlockNumberFor::<T>::from(5u32),
		);

		assert!(!ScheduledLimitOrderUpdates::<T>::get(BlockNumberFor::<T>::from(5u32)).is_empty());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
