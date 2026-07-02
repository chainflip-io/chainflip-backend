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
	Call, Config, MaxPendingWhitelistUpdates, MaxWithdrawalTimelock, Pallet, PalletConfigUpdate,
	WhitelistChange, WithdrawalWhitelists,
};
use cf_chains::{address::EncodedAddress, benchmarking_value::BenchmarkValue, AccountOrAddress};
use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, UnixTime};
use frame_system::{pallet_prelude::OriginFor, RawOrigin};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_pallet_config() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let update = PalletConfigUpdate::MaxWithdrawalTimelock { seconds: 1_000 };

		#[extrinsic_call]
		update_pallet_config(origin as OriginFor<T>, update);

		assert_eq!(MaxWithdrawalTimelock::<T>::get(), 1_000);
	}

	#[benchmark]
	fn update_whitelist() {
		let caller: T::AccountId = whitelisted_caller();
		let now = T::TimeSource::now().as_secs();
		let max = MaxPendingWhitelistUpdates::<T>::get();

		// Worst case: the restriction is on and the pending queue is nearly full, so the new change
		// is scheduled (not applied immediately) and the pending-count walk covers a full queue.
		WithdrawalWhitelists::<T>::mutate(&caller, |whitelist| {
			whitelist.set_timelock(1_000, now);
			for _ in 0..max.saturating_sub(1) {
				whitelist.schedule_change(
					WhitelistChange::Allow(AccountOrAddress::ExternalAddress(
						BenchmarkValue::benchmark_value(),
					)),
					now,
					max,
				);
			}
		});

		let change = WhitelistChange::Allow(AccountOrAddress::ExternalAddress(
			EncodedAddress::benchmark_value(),
		));

		#[extrinsic_call]
		update_whitelist(RawOrigin::Signed(caller.clone()), change);

		assert!(WithdrawalWhitelists::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn set_withdrawal_timelock() {
		let caller: T::AccountId = whitelisted_caller();
		let now = T::TimeSource::now().as_secs();

		// Worst case: an existing timelock that we lower — weakening is delayed, so this exercises
		// the pending-timelock path (read-modify-write of the stored whitelist).
		WithdrawalWhitelists::<T>::mutate(&caller, |whitelist| {
			whitelist.set_timelock(2_000, now);
		});

		#[extrinsic_call]
		set_withdrawal_timelock(RawOrigin::Signed(caller.clone()), 1_000);

		assert!(WithdrawalWhitelists::<T>::contains_key(&caller));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
