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
use frame_support::{
	assert_ok,
	sp_runtime::traits::One,
	traits::{EnsureOrigin, OnInitialize, UnfilteredDispatchable},
};
use sp_std::vec;

const SUPPLY_UPDATE_INTERVAL: u32 = 100;
const INFLATION_RATE: u32 = 200;

fn on_initialize_setup<T: Config>(should_mint: bool) -> BlockNumberFor<T> {
	use frame_support::sp_runtime::{traits::BlockNumberProvider, Digest};

	let pre_digest = Digest { logs: vec![] };
	frame_system::Pallet::<T>::initialize(
		&(frame_system::Pallet::<T>::current_block_number() + One::one()),
		&frame_system::Pallet::<T>::parent_hash(),
		&pre_digest,
	);

	if should_mint {
		SupplyUpdateInterval::<T>::get() + 1u32.into()
	} else {
		1u32.into()
	}
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_current_authority_emission_inflation() {
		let call =
			Call::<T>::update_current_authority_emission_inflation { inflation: INFLATION_RATE };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(CurrentAuthorityEmissionInflation::<T>::get(), INFLATION_RATE);
	}

	#[benchmark]
	fn rewards_minted() {
		let block_number = on_initialize_setup::<T>(true);

		#[block]
		{
			Pallet::<T>::on_initialize(block_number);
		}
	}

	#[benchmark]
	fn rewards_not_minted() {
		let block_number = on_initialize_setup::<T>(false);

		#[block]
		{
			Pallet::<T>::on_initialize(block_number);
		}
	}

	#[benchmark]
	fn update_supply_update_interval() {
		let call =
			Call::<T>::update_supply_update_interval { value: SUPPLY_UPDATE_INTERVAL.into() };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		let supply_update_interval = Pallet::<T>::supply_update_interval();
		assert_eq!(supply_update_interval, (100_u32).into());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
