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

//! Autogenerated weights for pallet_cf_flip
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-08-26, STEPS: `20`, REPEAT: `10`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `ip-172-31-10-39`, CPU: `Intel(R) Xeon(R) Platinum 8124M CPU @ 3.00GHz`
//! EXECUTION: , WASM-EXECUTION: Compiled, CHAIN: Some("dev-3"), DB CACHE: 1024

// Executed Command:
// ./chainflip-node
// benchmark
// pallet
// --pallet
// pallet_cf_flip
// --extrinsic
// *
// --output
// state-chain/pallets/cf-flip/src/weights.rs
// --steps=20
// --repeat=10
// --template=state-chain/chainflip-weight-template.hbs
// --chain=dev-3

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet_cf_flip.
pub trait WeightInfo {
	fn set_slashing_rate() -> Weight;
	fn reap_one_account() -> Weight;
}

/// Weights for pallet_cf_flip using the Substrate node and recommended hardware.
pub struct PalletWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for PalletWeight<T> {
	/// Storage: `Flip::SlashingRate` (r:0 w:1)
	/// Proof: `Flip::SlashingRate` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn set_slashing_rate() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_142_000 picoseconds.
		Weight::from_parts(6_545_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn reap_one_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `649`
		//  Estimated: `3545`
		// Minimum execution time: 17_855_000 picoseconds.
		Weight::from_parts(18_272_000, 3545)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	/// Storage: `Flip::SlashingRate` (r:0 w:1)
	/// Proof: `Flip::SlashingRate` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn set_slashing_rate() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_142_000 picoseconds.
		Weight::from_parts(6_545_000, 0)
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn reap_one_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `649`
		//  Estimated: `3545`
		// Minimum execution time: 17_855_000 picoseconds.
		Weight::from_parts(18_272_000, 3545)
			.saturating_add(ParityDbWeight::get().reads(2_u64))
			.saturating_add(ParityDbWeight::get().writes(2_u64))
	}
}
