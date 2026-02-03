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
	evm::api::{EvmEnvironmentProvider, EvmReplayProtection},
	ReplayProtectionProvider,
};
use cf_primitives::chains::{assets::bsc, Bsc};

use crate::{
	benchmarking_value::BenchmarkValue,
	evm::{
		api::{set_agg_key_with_agg_key::SetAggKeyWithAggKey, EvmTransactionBuilder},
		AggKey,
	},
};

use super::{api::BscApi, BscTrackedData};

impl BenchmarkValue for BscTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 5_000_000_000 }
	}
}

impl BenchmarkValue for bsc::Asset {
	fn benchmark_value() -> Self {
		bsc::Asset::BscBnb
	}
}

impl<E: ReplayProtectionProvider<Bsc> + EvmEnvironmentProvider<Bsc>> BenchmarkValue for BscApi<E> {
	fn benchmark_value() -> Self {
		EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed([2u8; 33])),
		)
		.into()
	}
}
