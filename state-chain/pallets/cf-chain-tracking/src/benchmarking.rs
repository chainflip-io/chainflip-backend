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

use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_chain_state() {
		let genesis_chain_state = ChainState {
			block_height: 1u32.into(),
			tracked_data: BenchmarkValue::benchmark_value(),
		};
		let new_chain_state = ChainState {
			block_height: 32u32.into(),
			tracked_data: BenchmarkValue::benchmark_value(),
		};

		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();
		// Dispatch once to ensure we have a value already inserted - replacing a value is more
		// expensive than inserting a new one.
		assert_ok!(Call::<T, I>::update_chain_state { new_chain_state: genesis_chain_state }
			.dispatch_bypass_filter(origin.clone()));

		#[block]
		{
			assert_ok!(Call::<T, I>::update_chain_state {
				new_chain_state: new_chain_state.clone()
			}
			.dispatch_bypass_filter(origin));
		}

		assert_eq!(CurrentChainState::<T, I>::get().unwrap(), new_chain_state);
	}

	#[cfg(test)]
	use crate::mock::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_update_chain_state::<Test, ()>(true);
		});
	}
}
