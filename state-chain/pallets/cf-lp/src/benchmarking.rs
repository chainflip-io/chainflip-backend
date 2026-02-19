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
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue};
use cf_primitives::{AccountRole, Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::{AccountRoleRegistry, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OnNewAccount};
use frame_system::RawOrigin;

#[benchmarks(
	where <T::FeePayment as cf_traits::FeePayment>::Amount: From<u128>
)]
mod benchmarks {
	use super::*;
	use frame_support::sp_runtime::FixedU128;
	use sp_std::vec::Vec;

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
	fn schedule_swap() {
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
		Pallet::<T>::schedule_swap(
			caller,
			1000,
			Asset::Eth,
			Asset::Flip,
			0,
			Default::default(),
			None,
		);
	}

	#[benchmark]
	fn update_agg_stats_existing(m: Linear<0, 100>) {
		use sp_std::collections::btree_map::BTreeMap;

		// Generate m LPs with existing aggregate stats
		let existing_lps = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::LiquidityProvider,
			m,
		)
		.unwrap();

		// Populate LpAggStats with existing LPs
		let mut agg_stats_map: BTreeMap<T::AccountId, BTreeMap<Asset, pallet::AggStats>> =
			BTreeMap::new();
		for lp in &existing_lps {
			let mut lp_stats: BTreeMap<Asset, pallet::AggStats> = BTreeMap::new();
			lp_stats.insert(
				Asset::Eth,
				pallet::AggStats::new(pallet::DeltaStats {
					limit_orders_swap_usd_volume: FixedU128::from_u32(100),
				}),
			);
			agg_stats_map.insert(lp.clone(), lp_stats);
		}
		pallet::LpAggStats::<T>::put(agg_stats_map);

		// Populate LpDeltaStats for existing LPs (they will be updated)
		for lp in &existing_lps {
			pallet::LpDeltaStats::<T>::insert(
				lp,
				Asset::Eth,
				pallet::DeltaStats { limit_orders_swap_usd_volume: FixedU128::from_u32(50) },
			);
		}

		#[block]
		{
			Pallet::<T>::update_agg_stats(Weight::MAX);
		}

		// Verify existing LPs had their stats updated
		let updated_agg_stats = pallet::LpAggStats::<T>::get();
		for lp in &existing_lps {
			assert!(updated_agg_stats.contains_key(lp));
		}
		// Verify delta stats were drained
		assert_eq!(pallet::LpDeltaStats::<T>::iter().count(), 0);
	}

	#[benchmark]
	fn update_agg_stats_new(n: Linear<0, 100>) {
		use sp_std::collections::btree_map::BTreeMap;

		// Generate n LPs that only have delta stats (new LPs)
		let new_lps = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::LiquidityProvider,
			n,
		)
		.unwrap();

		// Ensure LpAggStats is empty
		pallet::LpAggStats::<T>::put(
			BTreeMap::<T::AccountId, BTreeMap<Asset, pallet::AggStats>>::new(),
		);

		// Populate LpDeltaStats for new LPs (they will be inserted as new agg entries)
		for lp in &new_lps {
			pallet::LpDeltaStats::<T>::insert(
				lp,
				Asset::Eth,
				pallet::DeltaStats { limit_orders_swap_usd_volume: FixedU128::from_u32(25) },
			);
		}

		#[block]
		{
			Pallet::<T>::update_agg_stats(Weight::MAX);
		}

		// Verify new LPs were added to agg stats
		let updated_agg_stats = pallet::LpAggStats::<T>::get();
		for lp in &new_lps {
			assert!(updated_agg_stats.contains_key(lp));
		}
		// Verify delta stats were drained
		assert_eq!(pallet::LpDeltaStats::<T>::iter().count(), 0);
	}

	#[benchmark]
	fn purge_balances(n: Linear<1, 100>) {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let account_ids = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::LiquidityProvider,
			n as u32,
		)
		.unwrap();

		for account_id in &account_ids {
			assert_ok!(Pallet::<T>::register_liquidity_refund_address(
				RawOrigin::Signed(account_id.clone()).into(),
				EncodedAddress::Eth(Default::default()),
			));
		}

		let accounts_to_purge = account_ids
			.into_iter()
			.enumerate()
			.map(|(i, account_id)| {
				let asset = match i % 3 {
					0 => Asset::Eth,
					1 => Asset::Flip,
					_ => Asset::Usdc,
				};
				T::BalanceApi::credit_account(&account_id, asset, 1_000_000_000);
				(account_id, asset, 500_000_000)
			})
			.collect::<Vec<_>>();

		#[block]
		{
			assert_ok!(Pallet::<T>::purge_balances(origin, accounts_to_purge));
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
