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

use crate::Pallet;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_traits::EpochInfo;
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const TX_HASH: [u8; 32] = [0xab; 32];

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn vault_key_rotated_externally() {
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let call = Call::<T, I>::vault_key_rotated_externally {
			new_public_key: AggKeyFor::<T, I>::benchmark_value(),
			block_number: 5u32.into(),
			tx_id: Decode::decode(&mut &TX_HASH[..]).unwrap(),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(VaultStartBlockNumbers::<T, I>::contains_key(
			T::EpochInfo::epoch_index().saturating_add(1)
		));
	}

	#[benchmark]
	fn initialize_chain() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T, I>::initialize_chain {};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(ChainInitialized::<T, I>::get());
	}

	#[cfg(test)]
	use crate::mock::{new_test_ext, Test};

	impl_benchmark_test_suite!(Pallet, new_test_ext(), Test);
}
