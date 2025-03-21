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

use cf_primitives::{AccountRole, Asset};
use cf_traits::{AccountRoleRegistry, PoolTouched};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize() {
		let whitelisted_callers = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::LiquidityProvider,
			50,
		)
		.unwrap();

		for account in whitelisted_callers {
			for _ in 0..50 {
				CallCounter::<T>::mutate(
					CallInfoId::Pool(PoolTouched {
						account: account.clone(),
						base_asset: Asset::Eth,
					}),
					|count| {
						*count += 1;
					},
				);
			}
		}

		#[block]
		{
			Pallet::<T>::on_initialize(0u32.into());
		}
	}

	#[benchmark]
	fn update_pallet_config() {
		let slashing_rate: Permill = Permill::one();
		let call = Call::<T>::update_pallet_config {
			updates: vec![PalletConfigUpdate::SetSlashingRate(slashing_rate)].try_into().unwrap(),
		};
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(Pallet::<T>::slashing_rate(), slashing_rate)
	}

	#[benchmark]
	fn reap_one_account() {
		let caller: T::AccountId = whitelisted_caller();
		Account::<T>::insert(
			&caller,
			FlipAccount { balance: T::Balance::from(0u32), bond: T::Balance::from(0u32) },
		);

		#[block]
		{
			BurnFlipAccount::<T>::on_killed_account(&caller);
		}

		assert!(!Account::<T>::contains_key(&caller));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
