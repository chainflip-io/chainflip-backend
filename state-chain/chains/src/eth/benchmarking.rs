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

use crate::{
	benchmarking_value::BenchmarkValue,
	evm::api::{EvmReplayProtection, EvmTransactionBuilder},
};

use super::{
	api::{update_flip_supply::UpdateFlipSupply, EthereumApi},
	EthereumTrackedData,
};

impl<E> BenchmarkValue for EthereumApi<E> {
	fn benchmark_value() -> Self {
		EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			UpdateFlipSupply::new(1000000u128, 1u64),
		)
		.into()
	}
}

impl BenchmarkValue for EthereumTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 10_000_000_000, priority_fee: 2_000_000_000 }
	}
}
