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

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn clear_events() {
		let event = CfeEvent::<T>::EvmKeygenRequest(KeygenRequest::<T> {
			ceremony_id: 0,
			epoch_index: 0,
			participants: Default::default(),
		});

		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event);

		#[block]
		{
			CfeEvents::<T>::kill();
		}

		assert!(CfeEvents::<T>::get().is_empty());
	}
}
