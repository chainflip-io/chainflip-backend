#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::common::price_at_tick;
use cf_chains::ForeignChainAddress;
use cf_primitives::Asset;
use cf_traits::{AccountRoleRegistry, LpBalanceApi};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	assert_ok,
	dispatch::UnfilteredDispatchable,
	sp_runtime::traits::One,
	traits::{EnsureOrigin, OnNewAccount},
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

benchmarks! {
	update_buy_interval {
		let call = Call::<T>::update_buy_interval{
			new_buy_interval: BlockNumberFor::<T>::one(),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert_eq!(FlipBuyInterval::<T>::get(), BlockNumberFor::<T>::one());
	}

	update_pool_enabled {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let _ = Pallet::<T>::new_pool(origin, Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap());
		let call =  Call::<T>::update_pool_enabled{
			base_asset: Asset::Eth,
			pair_asset: Asset::Usdc,
			enabled: false,
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert!(!Pools::<T>::get(CanonicalAssetPair::new(Asset::Eth, STABLE_ASSET).unwrap()).unwrap().enabled);
	}

	new_pool {
		let call =  Call::<T>::new_pool {
			base_asset: Asset::Eth,
			pair_asset: Asset::Usdc,
			fee_hundredth_pips: 0u32,
			initial_price: price_at_tick(0).unwrap(),
		};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap());
	} verify {
		assert!(Pools::<T>::get(CanonicalAssetPair::new(Asset::Eth, STABLE_ASSET).unwrap()).is_some());
	}

	update_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
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
		Asset::Usdc,
		0,
		Some(-100..100),
		IncreaseOrDecrease::Increase(
			RangeOrderSize::AssetAmounts {
				maximum: AssetAmounts {
					base: 1_000_000,
					pair: 1_000_000,
				},
				minimum: AssetAmounts {
					base: 500_000,
					pair: 500_000,
				},
			}
		)
	)
	verify {}

	set_range_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
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
		Asset::Usdc,
		0,
		Some(-100..100),
		RangeOrderSize::AssetAmounts {
			maximum: AssetAmounts {
				base: 1_000_000,
				pair: 1_000_000,
			},
			minimum: AssetAmounts {
				base: 500_000,
				pair: 500_000,
			},
		}
	)
	verify {}

	update_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
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
		Asset::Usdc,
		0,
		Some(100),
		IncreaseOrDecrease::Increase(1_000_000)
	)
	verify {}

	set_limit_order {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
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
		Asset::Usdc,
		0,
		Some(100),
		1_000
	)
	verify {}

	set_pool_fees {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
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
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Usdc,
			Asset::Eth,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			Asset::Usdc,
			1,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::swap_with_network_fee(STABLE_ASSET, Asset::Eth, 1_000));
		let fee = 1_000;
		let call = Call::<T>::set_pool_fees {
			base_asset: Asset::Eth,
			pair_asset: Asset::Usdc,
			fee_hundredth_pips: fee,
		};
	}: { let _ = call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()); }
	verify {
		assert_eq!(
			Pallet::<T>::pool_info(Asset::Eth, STABLE_ASSET),
			Some(PoolInfo {
				limit_order_fee_hundredth_pips: fee,
				range_order_fee_hundredth_pips: fee,
			})
		);
	}

	// TODO: This benchmark is failing right now but only during the real execution, not during testing.
	// mint_or_burn {
	// 	let caller = new_lp_account::<T>();
	// 	let a in 0..100;
	// 	let mint_block = BlockNumberFor::<T>::from(1_u32);
	// 	let expire_block = BlockNumberFor::<T>::from(100_u32);
	// 	assert_ok!(Pallet::<T>::new_pool(T::EnsureGovernance::try_successful_origin().unwrap(), Asset::Eth, Asset::Usdc, 0, price_at_tick(0).unwrap()));
	// 	assert_ok!(T::LpBalance::try_credit_account(
	// 		&caller,
	// 		Asset::Eth,
	// 		1_000_000,
	// 	));
	// 	assert_ok!(T::LpBalance::try_credit_account(
	// 		&caller,
	// 		Asset::Usdc,
	// 		1_000_000,
	// 	));
	// 	for i in 0..a {
	// 		LimitOrderQueue::<T>::append(mint_block, OrderUpdate::Mint {
	// 			order_details: LimitOrderDetails {
	// 				lp: caller.clone(),
	// 				sell_asset: Asset::Eth,
	// 				buy_asset: Asset::Usdc,
	// 				id: i as u64,
	// 				option_tick: Some(0),
	// 				sell_amount: 1_000,
	// 			},
	// 			expiry_block: Some(expire_block),
	// 		});
	// 	}
	// } : {
	// 	Pallet::<T>::mint_or_burn(mint_block);
	// } verify {
	// 	assert!(LimitOrderQueue::<T>::get(mint_block).is_empty());
	// 	assert_eq!(LimitOrderQueue::<T>::get(expire_block).len(), a as usize);
	// }

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test,
	);
}
