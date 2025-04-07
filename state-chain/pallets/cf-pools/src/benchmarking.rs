// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_amm::math::price_at_tick;
use cf_chains::ForeignChainAddress;
use cf_primitives::{AccountRole, Asset};
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};

fn new_lp_account<T: Chainflip + Config>() -> T::AccountId {
	let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
		AccountRole::LiquidityProvider,
	)
	.unwrap();
	for address in [
		ForeignChainAddress::Eth(Default::default()),
		ForeignChainAddress::Dot(Default::default()),
		ForeignChainAddress::Btc(cf_chains::btc::ScriptPubkey::P2PKH(Default::default())),
	] {
		T::LpRegistrationApi::register_liquidity_refund_address(&caller, address);
	}
	caller
}

#[benchmarks]
mod benchmarks {
	use super::*;

	// Create some orders so the cost of sweeping is taken into account
	fn create_some_orders<T: Config>(caller: T::AccountId) {
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000_000);

		for i in 1..=3 {
			assert_ok!(Pallet::<T>::set_range_order(
				RawOrigin::Signed(caller.clone()).into(),
				Asset::Eth,
				Asset::Usdc,
				i as u64,
				Some(-i..i),
				RangeOrderSize::AssetAmounts {
					maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
					minimum: AssetAmounts { base: 500_000, quote: 500_000 },
				},
			));
		}
		for i in 1_u64..=10 {
			assert_ok!(Pallet::<T>::set_limit_order(
				RawOrigin::Signed(caller.clone()).into(),
				Asset::Eth,
				Asset::Usdc,
				Side::Buy,
				i,
				Some(0),
				1_000,
			));
		}
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);

		create_some_orders::<T>(caller.clone());

		#[extrinsic_call]
		update_range_order(
			RawOrigin::Signed(caller),
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);

		create_some_orders::<T>(caller.clone());

		#[extrinsic_call]
		set_range_order(
			RawOrigin::Signed(caller),
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);

		create_some_orders::<T>(caller.clone());

		#[extrinsic_call]
		update_limit_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			Side::Sell,
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);

		create_some_orders::<T>(caller.clone());

		#[extrinsic_call]
		set_limit_order(
			RawOrigin::Signed(caller.clone()),
			Asset::Eth,
			Asset::Usdc,
			Side::Sell,
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			Asset::Usdc,
			Side::Buy,
			0,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::set_limit_order(
			RawOrigin::Signed(caller.clone()).into(),
			Asset::Eth,
			Asset::Usdc,
			Side::Sell,
			1,
			Some(0),
			10_000,
		));
		assert_ok!(Pallet::<T>::swap_single_leg(STABLE_ASSET, Asset::Eth, 1_000));
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

		match Pallet::<T>::pool_info(Asset::Eth, STABLE_ASSET) {
			Ok(pool_info) => {
				assert_eq!(pool_info.range_order_fee_hundredth_pips, fee);
			},
			Err(_) => panic!("Pool not found"),
		}
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
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000);
		#[extrinsic_call]
		schedule_limit_order_update(
			RawOrigin::Signed(caller.clone()),
			Box::new(Call::<T>::set_limit_order {
				base_asset: Asset::Eth,
				quote_asset: Asset::Usdc,
				side: Side::Sell,
				id: 0,
				option_tick: Some(0),
				sell_amount: 100,
			}),
			BlockNumberFor::<T>::from(5u32),
		);

		assert!(!ScheduledLimitOrderUpdates::<T>::get(BlockNumberFor::<T>::from(5u32)).is_empty());
	}

	#[benchmark]
	fn set_maximum_price_impact(n: Linear<1, 6>) {
		const LIMIT: u32 = 1_000;
		let limits = Asset::all()
			.filter(|asset| *asset != STABLE_ASSET)
			.take(n as usize)
			.zip(sp_std::iter::repeat(Some(LIMIT)))
			.collect::<Vec<_>>()
			.try_into()
			.unwrap();
		let call = Call::<T>::set_maximum_price_impact { limits };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		for (i, asset) in Asset::all().filter(|asset| *asset != STABLE_ASSET).enumerate() {
			let asset_pair =
				AssetPair::try_new::<T>(asset, STABLE_ASSET).expect("Asset Pair must succeed");
			if (i as u32) < n {
				assert_eq!(MaximumPriceImpact::<T>::get(asset_pair), Some(LIMIT));
			} else {
				assert_eq!(MaximumPriceImpact::<T>::get(asset_pair), None);
			}
		}
	}

	#[benchmark]
	fn cancel_orders_batch(n: Linear<1, 100>) {
		let caller = new_lp_account::<T>();
		assert_ok!(Pallet::<T>::new_pool(
			T::EnsureGovernance::try_successful_origin().unwrap(),
			Asset::Eth,
			Asset::Usdc,
			0,
			price_at_tick(0).unwrap()
		));
		T::LpBalance::credit_account(&caller, Asset::Eth, 1_000_000_000);
		T::LpBalance::credit_account(&caller, Asset::Usdc, 1_000_000_000);

		let orders_to_delete = (1i32..=n as i32)
			.map(|i| {
				assert_ok!(Pallet::<T>::set_range_order(
					RawOrigin::Signed(caller.clone()).into(),
					Asset::Eth,
					Asset::Usdc,
					i as u64,
					Some(-i..i),
					RangeOrderSize::AssetAmounts {
						maximum: AssetAmounts { base: 1_000_000, quote: 1_000_000 },
						minimum: AssetAmounts { base: 500_000, quote: 500_000 },
					},
				));
				CloseOrder::Range { base_asset: Asset::Eth, quote_asset: Asset::Usdc, id: i as u64 }
			})
			.collect::<Vec<_>>()
			.try_into()
			.unwrap();

		#[extrinsic_call]
		crate::benchmarking::benchmarks::cancel_orders_batch(
			RawOrigin::Signed(caller.clone()),
			orders_to_delete,
		);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
