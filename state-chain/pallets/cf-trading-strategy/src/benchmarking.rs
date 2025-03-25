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

use super::*;
use cf_chains::ForeignChainAddress;
use cf_primitives::Asset;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;

fn new_lp_account<T: Chainflip + Config>() -> T::AccountId {
	use cf_primitives::AccountRole;
	use cf_traits::AccountRoleRegistry;
	let caller = <T as Chainflip>::AccountRoleRegistry::whitelisted_caller_with_role(
		AccountRole::LiquidityProvider,
	)
	.unwrap();

	T::LpRegistrationApi::register_liquidity_refund_address(
		&caller,
		ForeignChainAddress::Eth(Default::default()),
	);

	caller
}

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[benchmarks]
mod benchmarks {
	use super::*;

	const ASSET: Asset = Asset::Usdt;

	fn turn_off_minimums<T: Config>() {
		let zero_thresholds = BTreeMap::from_iter([(ASSET, 0), (STABLE_ASSET, 0)]);
		MinimumDeploymentAmountForStrategy::<T>::set(zero_thresholds.clone());
		MinimumAddedFundsToStrategy::<T>::set(zero_thresholds);
	}

	#[benchmark]
	fn deploy_strategy() {
		let caller = new_lp_account::<T>();
		turn_off_minimums::<T>();

		T::BalanceApi::credit_account(&caller, ASSET, 1000);
		T::BalanceApi::credit_account(&caller, STABLE_ASSET, 1000);

		assert_eq!(Strategies::<T>::iter().count(), 0);

		assert_ok!(T::PoolApi::create_pool(ASSET, STABLE_ASSET, 0, 12345.into()));

		#[extrinsic_call]
		deploy_strategy(
			RawOrigin::Signed(caller.clone()),
			TradingStrategy::SellAndBuyAtTicks { sell_tick: 1, buy_tick: -1, base_asset: ASSET },
			BTreeMap::from_iter([(Asset::Usdt, 1000), (STABLE_ASSET, 1000)]),
		);

		assert_eq!(Strategies::<T>::iter().count(), 1);
	}

	#[benchmark]
	fn close_strategy() {
		let caller = new_lp_account::<T>();
		turn_off_minimums::<T>();

		T::BalanceApi::credit_account(&caller, ASSET, 1000);
		T::BalanceApi::credit_account(&caller, STABLE_ASSET, 1000);

		assert_ok!(T::PoolApi::create_pool(ASSET, STABLE_ASSET, 0, 12345.into()));

		assert_ok!(Pallet::<T>::deploy_strategy(
			RawOrigin::Signed(caller.clone()).into(),
			TradingStrategy::SellAndBuyAtTicks { sell_tick: 1, buy_tick: -1, base_asset: ASSET },
			BTreeMap::from_iter([(ASSET, 1000), (STABLE_ASSET, 1000)]),
		));

		let (_, strategy_id, _) = Strategies::<T>::iter().next().unwrap();

		// Calling on_idle manually to make sure limit orders are created:
		Pallet::<T>::on_idle(0u32.into(), Weight::MAX);

		#[extrinsic_call]
		close_strategy(RawOrigin::Signed(caller.clone()), strategy_id);

		assert_eq!(Strategies::<T>::iter().count(), 0);
	}

	#[benchmark]
	fn add_funds_to_strategy() {
		let caller = new_lp_account::<T>();
		turn_off_minimums::<T>();

		T::BalanceApi::credit_account(&caller, ASSET, 2000);
		T::BalanceApi::credit_account(&caller, STABLE_ASSET, 2000);

		assert_ok!(T::PoolApi::create_pool(ASSET, STABLE_ASSET, 0, 12345.into()));

		assert_ok!(Pallet::<T>::deploy_strategy(
			RawOrigin::Signed(caller.clone()).into(),
			TradingStrategy::SellAndBuyAtTicks { sell_tick: 1, buy_tick: -1, base_asset: ASSET },
			BTreeMap::from_iter([(ASSET, 1000), (STABLE_ASSET, 1000)]),
		));

		let (_, strategy_id, _) = Strategies::<T>::iter().next().unwrap();

		// Calling on_idle manually to make sure limit orders are created:
		Pallet::<T>::on_idle(0u32.into(), Weight::MAX);

		#[extrinsic_call]
		add_funds_to_strategy(
			RawOrigin::Signed(caller.clone()),
			strategy_id,
			BTreeMap::from_iter([(ASSET, 1000), (STABLE_ASSET, 1000)]),
		);
	}
}
