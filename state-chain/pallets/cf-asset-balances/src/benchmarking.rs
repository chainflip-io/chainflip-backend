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

use crate::{
	Call, Config, MaxPendingWhitelistUpdates, MaxWhitelistEntries, MaxWhitelistTimelock, Pallet,
	PalletConfigUpdate, PendingChange, PendingChanges, WhitelistChange,
};
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue, AccountOrAddress};
use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Hooks};
use frame_system::{pallet_prelude::OriginFor, RawOrigin};

#[benchmarks]
mod benchmarks {
	use super::*;
	use frame_support::pallet_prelude::Weight;

	fn pending_count<T: Config>(who: &T::AccountId) -> u32 {
		PendingChanges::<T>::get()
			.values()
			.flatten()
			.filter(|(account, _)| account == who)
			.count() as u32
	}

	fn pending_total<T: Config>() -> u32 {
		PendingChanges::<T>::get().values().flatten().count() as u32
	}

	fn target<T: Config>(i: u32) -> T::AccountId {
		account("target", i, 0)
	}

	#[benchmark]
	fn update_pallet_config() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let update = PalletConfigUpdate::MaxWhitelistTimelock { seconds: 1_000 };

		#[extrinsic_call]
		update_pallet_config(origin as OriginFor<T>, update);

		assert_eq!(MaxWhitelistTimelock::<T>::get(), 1_000);
	}

	#[benchmark]
	fn update_whitelist() {
		let caller: T::AccountId = whitelisted_caller();
		let max = MaxPendingWhitelistUpdates::<T>::get();

		// Worst case: the pending queue is nearly full, so the pending-count walk covers a full
		// queue.
		Pallet::<T>::mutate_whitelist(&caller, |whitelist| whitelist.set_timelock(1_000));
		for _ in 0..max.saturating_sub(1) {
			Pallet::<T>::schedule_or_apply_change(
				&caller,
				PendingChange::Whitelist(WhitelistChange::Allow(
					AccountOrAddress::ExternalAddress(BenchmarkValue::benchmark_value()),
				)),
				1_000,
			)
			.unwrap();
		}

		let change = WhitelistChange::Allow(AccountOrAddress::ExternalAddress(
			EncodedAddress::benchmark_value(),
		));

		#[extrinsic_call]
		update_whitelist(RawOrigin::Signed(caller.clone()), change);

		assert_eq!(pending_count::<T>(&caller), max);
	}

	#[benchmark]
	fn set_whitelist_timelock() {
		let caller: T::AccountId = whitelisted_caller();

		// Update an existing timelock, exercising the scheduling path.
		Pallet::<T>::mutate_whitelist(&caller, |whitelist| whitelist.set_timelock(2_000));

		#[extrinsic_call]
		set_whitelist_timelock(RawOrigin::Signed(caller.clone()), 1_000);

		assert!(pending_count::<T>(&caller) > 0);
	}

	#[benchmark]
	fn on_idle_check() {
		// The no-work path: pending changes exist but none are due, so `on_idle` returns after a
		// single read.
		let caller: T::AccountId = whitelisted_caller();
		Pallet::<T>::mutate_whitelist(&caller, |whitelist| whitelist.set_timelock(1_000));
		Pallet::<T>::schedule_or_apply_change(&caller, PendingChange::Timelock(0), 1_000).unwrap();

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		assert!(pending_count::<T>(&caller) > 0);
	}

	#[benchmark]
	fn on_idle_apply_change(n: Linear<1, 100>) {
		let max_entries = MaxWhitelistEntries::<T>::get();

		// Each change must target a *distinct* account: `apply_pending_change` mutates
		// `WithdrawalWhitelists` per account, so sharing one account across all `n` would let the
		// storage overlay amortise that read/write into the base weight and leave the per-change
		// term with no db cost at all. Each target is also one entry short of full, so the change
		// is applied to a max-sized whitelist.
		for i in 0..n {
			Pallet::<T>::mutate_whitelist(&target::<T>(i), |whitelist| {
				whitelist.set_timelock(1_000);
				for j in 0..max_entries.saturating_sub(1) {
					whitelist
						.apply_change(
							&WhitelistChange::Allow(AccountOrAddress::InternalAccount(account(
								"filler", i, j,
							))),
							max_entries,
						)
						.unwrap();
				}
			});
		}

		// Insert the matured changes directly, bypassing the per-account pending cap.
		PendingChanges::<T>::mutate(|pending| {
			pending.entry(0).or_default().extend((0..n).map(|i| {
				(
					target::<T>(i),
					PendingChange::Whitelist(WhitelistChange::Allow(
						AccountOrAddress::ExternalAddress(BenchmarkValue::benchmark_value()),
					)),
				)
			}));
		});
		assert_eq!(pending_total::<T>(), n);

		#[block]
		{
			Pallet::<T>::on_idle(Default::default(), Weight::MAX);
		}

		assert_eq!(pending_total::<T>(), 0);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
