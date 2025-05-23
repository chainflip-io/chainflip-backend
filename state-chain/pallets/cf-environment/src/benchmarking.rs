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

use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_safe_mode() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::update_safe_mode { update: SafeModeUpdate::CodeRed };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(RuntimeSafeMode::<T>::get(), SafeMode::CODE_RED);
	}

	#[benchmark]
	fn update_consolidation_parameters() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call =
			Call::<T>::update_consolidation_parameters { params: INITIAL_CONSOLIDATION_PARAMETERS };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(ConsolidationParameters::<T>::get(), INITIAL_CONSOLIDATION_PARAMETERS);
	}

	#[benchmark]
	fn force_recover_sol_nonce() {
		let nonce_account = SolAddress([0x01; 32]);
		let old_hash = SolHash([0x02; 32]);
		let new_hash = SolHash([0x10; 32]);

		// Setup unavailable Nonce
		SolanaUnavailableNonceAccounts::<T>::insert(nonce_account, old_hash);

		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call =
			Call::<T>::force_recover_sol_nonce { nonce_account, durable_nonce: Some(new_hash) };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(SolanaAvailableNonceAccounts::<T>::get().contains(&(nonce_account, new_hash)));
		assert!(SolanaUnavailableNonceAccounts::<T>::get(nonce_account).is_none());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
