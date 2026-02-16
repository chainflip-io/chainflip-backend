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
	evm::{
		api::{
			set_agg_key_with_agg_key::SetAggKeyWithAggKey, EvmReplayProtection,
			EvmTransactionBuilder,
		},
		AggKey,
	},
	ReplayProtectionProvider,
};
use cf_primitives::chains::Tron;

use super::{
	api::TronApi, evm::api::EvmEnvironmentProvider, TronTrackedData, TronTransaction,
	TronTransactionFee, TronTransactionMetadata,
};

impl BenchmarkValue for TronTrackedData {
	fn benchmark_value() -> Self {
		Self {}
	}
}

impl BenchmarkValue for Tron {
	fn benchmark_value() -> Self {
		Tron
	}
}

impl BenchmarkValue for TronTransactionMetadata {
	fn benchmark_value() -> Self {
		use crate::evm::Address;
		Self { contract: Address::zero(), fee_limit: None }
	}
}

impl BenchmarkValue for TronTransactionFee {
	fn benchmark_value() -> Self {
		Self {
			fee: 1_000_000,
			energy_usage: 50_000,
			energy_fee: 500_000,
			origin_energy_usage: 0,
			energy_usage_total: 50_000,
			net_usage: 300,
			net_fee: 500_000,
			energy_penalty_total: 0,
		}
	}
}

impl BenchmarkValue for TronTransaction {
	fn benchmark_value() -> Self {
		use crate::evm::Address;
		Self {
			chain_id: super::CHAIN_ID_MAINNET,
			fee_limit: None,
			contract: Address::zero(),
			value: 0u64.into(),
			data: b"do_something()".to_vec(),
		}
	}
}

impl<E: ReplayProtectionProvider<Tron> + EvmEnvironmentProvider<Tron>> BenchmarkValue
	for TronApi<E>
{
	fn benchmark_value() -> Self {
		EvmTransactionBuilder::new_unsigned(
			EvmReplayProtection::default(),
			SetAggKeyWithAggKey::new(AggKey::from_pubkey_compressed([2u8; 33])),
		)
		.into()
	}
}
