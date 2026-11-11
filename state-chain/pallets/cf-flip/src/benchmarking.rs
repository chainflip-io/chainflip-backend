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

use cf_primitives::{AccountRole, Asset, FLIPPERINOS_PER_FLIP};
use cf_traits::AccountRoleRegistry;
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
					OpaqueCallIndex::<T>(
						(account.clone(), Asset::Eth).encode(),
						Default::default(),
					),
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
	fn continue_reward_distribution(p: Linear<1, 1_000>, n: Linear<1_000, 20_000>) {
		const EPOCH: EpochIndex = 1;
		T::RewardsDistribution::setup_benchmark_payees(EPOCH, n);

		let total: T::Balance = (u128::from(n) * FLIPPERINOS_PER_FLIP).into();
		TotalIssuance::<T>::mutate(|issuance| *issuance += total);
		OffchainFunds::<T>::mutate(|funds| *funds += total);
		Pallet::<T>::bridge_in_to_distribution_reserve(total);
		PendingRewardDistribution::<T>::put(RewardDistributionProgress {
			epoch_index: EPOCH,
			total,
			undistributed: total,
			resume_from: None,
		});

		// Only payees have a cost, and it dwarfs that of storage reads, so exactly `p` of them fit.
		const PAYEE_WEIGHT: u64 = 1_000_000_000_000;
		let mut meter = WeightMeter::with_limit(Weight::from_parts(
			u64::from(p) * PAYEE_WEIGHT + PAYEE_WEIGHT / 2,
			u64::MAX,
		));

		let funded_accounts =
			|| Account::<T>::iter_values().filter(|account| !account.total().is_zero()).count();
		let funded_before = funded_accounts();

		#[block]
		{
			Pallet::<T>::continue_reward_distribution(&mut meter, |payees, _| {
				Weight::from_parts(u64::from(payees) * PAYEE_WEIGHT, 0)
			});
		}

		assert_eq!(funded_accounts() - funded_before, p as usize);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
