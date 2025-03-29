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

use crate::benchmarking_value::BenchmarkValue;

use super::{
	api::{rotate_vault_proxy, AssethubApi},
	dot::{PolkadotReplayProtection, RuntimeVersion},
	AssethubTrackedData,
};

impl<E> BenchmarkValue for AssethubApi<E> {
	fn benchmark_value() -> Self {
		AssethubApi::RotateVaultProxy(rotate_vault_proxy::extrinsic_builder(
			PolkadotReplayProtection {
				genesis_hash: Default::default(),
				signer: BenchmarkValue::benchmark_value(),
				nonce: Default::default(),
			},
			Some(Default::default()),
			Default::default(),
			Default::default(),
		))
	}
}

impl BenchmarkValue for AssethubTrackedData {
	fn benchmark_value() -> Self {
		AssethubTrackedData {
			median_tip: 2,
			runtime_version: RuntimeVersion { spec_version: 17, transaction_version: 16 },
		}
	}
}
