// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2022-07-05 (Y/M/D)
//!
//! DATABASE: `RocksDb`, RUNTIME: `Three node testnet`
//! BLOCK-NUM: `BlockId::Number(0)`
//! SKIP-WRITE: `false`, SKIP-READ: `false`, WARMUPS: `1`
//! STATE-VERSION: `V1`, STATE-CACHE-SIZE: `0`
//! WEIGHT-PATH: `./state-chain/runtime/src/weights`
//! METRIC: `Average`, WEIGHT-MUL: `1`, WEIGHT-ADD: `0`

// Executed Command:
// ./target/release/chainflip-node benchmark storage --state-version 1 --chain three-node-test
// --database rocksdb --weight-path ./state-chain/runtime/src/weights

/// Storage DB weights for the `Three node testnet` runtime and `RocksDb`.
pub mod constants {
	use frame_support::{
		parameter_types,
		weights::{constants, RuntimeDbWeight},
	};

	parameter_types! {
		/// By default, Substrate uses `RocksDB`, so this will be the weight used throughout
		/// the runtime.
		pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
			/// Time to read one storage item.
			/// Calculated by multiplying the *Average* of all values with `1` and adding `0`.
			///
			/// Stats [NS]:
			///   Min, Max: 6_625, 640_333
			///   Average:  15_586
			///   Median:   9_541
			///   Std-Dev:  60419.47
			///
			/// Percentiles [NS]:
			///   99th: 13_625
			///   95th: 13_041
			///   75th: 10_958
			read: 15_586 * constants::WEIGHT_PER_NANOS.ref_time(),

			/// Time to write one storage item.
			/// Calculated by multiplying the *Average* of all values with `1` and adding `0`.
			///
			/// Stats [NS]:
			///   Min, Max: 15_666, 3_633_666
			///   Average:  59_806
			///   Median:   24_583
			///   Std-Dev:  345672.53
			///
			/// Percentiles [NS]:
			///   99th: 125_875
			///   95th: 35_916
			///   75th: 28_875
			write: 59_806 * constants::WEIGHT_PER_NANOS.ref_time(),
		};
	}

	#[cfg(test)]
	mod test_db_weights {

		/// Checks that all weights exist and have sane values.
		// NOTE: If this test fails but you are sure that the generated values are fine,
		// you can delete it.
		#[test]
		fn bound() {
			// At least 1 µs.
			// assert!(
			// 	W::get().reads(1) >= constants::WEIGHT_PER_MICROS,
			// 	"Read weight should be at least 1 µs."
			// );
			// assert!(
			// 	W::get().writes(1) >= constants::WEIGHT_PER_MICROS,
			// 	"Write weight should be at least 1 µs."
			// );
			// // At most 1 ms.
			// assert!(
			// 	W::get().reads(1) <= constants::WEIGHT_PER_MILLIS,
			// 	"Read weight should be at most 1 ms."
			// );
			// assert!(
			// 	W::get().writes(1) <= constants::WEIGHT_PER_MILLIS,
			// 	"Write weight should be at most 1 ms."
			// );
		}
	}
}
