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

use super::{
	api::{SolanaApi, VaultSwapAccountAndSender},
	SolAddress, SolHash, SolLegacyMessage, SolLegacyTransaction, SolSignature, SolTrackedData,
	SolanaTransactionData,
};

use crate::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};

impl BenchmarkValue for SolAddress {
	fn benchmark_value() -> Self {
		[1u8; 32].into()
	}
}

impl BenchmarkValueExtended for SolAddress {
	fn benchmark_value_by_id(id: u8) -> Self {
		[id; 32].into()
	}
}

impl BenchmarkValue for SolTrackedData {
	fn benchmark_value() -> Self {
		SolTrackedData { priority_fee: 100_000 }
	}
}

impl BenchmarkValue for SolLegacyMessage {
	fn benchmark_value() -> Self {
		Self::new_with_blockhash(&[], None, &SolHash::default().into())
	}
}

impl BenchmarkValue for SolLegacyTransaction {
	fn benchmark_value() -> Self {
		SolLegacyTransaction::new_unsigned(SolLegacyMessage::benchmark_value())
	}
}

impl BenchmarkValue for SolanaTransactionData {
	fn benchmark_value() -> Self {
		SolanaTransactionData {
			serialized_transaction: SolLegacyTransaction::benchmark_value()
				.finalize_and_serialize()
				.expect("Failed to serialize payload"),
			skip_preflight: false,
		}
	}
}

impl BenchmarkValue for SolSignature {
	fn benchmark_value() -> Self {
		[4u8; 64].into()
	}
}

impl BenchmarkValue for SolHash {
	fn benchmark_value() -> Self {
		[5u8; 32].into()
	}
}

impl<E: crate::sol::api::SolanaEnvironment> BenchmarkValue for SolanaApi<E> {
	fn benchmark_value() -> Self {
		SolanaApi::<E>::rotate_agg_key([8u8; 32].into())
			.expect("Benchmark value for SolApi must work.")
	}
}

impl BenchmarkValue for VaultSwapAccountAndSender {
	fn benchmark_value() -> Self {
		Self {
			swap_sender: BenchmarkValue::benchmark_value(),
			vault_swap_account: BenchmarkValue::benchmark_value(),
		}
	}
}
