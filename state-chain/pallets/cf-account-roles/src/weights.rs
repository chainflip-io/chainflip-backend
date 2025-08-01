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


//! Autogenerated weights for pallet_cf_account_roles
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
// pallet_cf_account_roles
// --extrinsic
// *
// --output
// state-chain/pallets/cf-account-roles/src/weights.rs
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

/// Weight functions needed for pallet_cf_account_roles.
pub trait WeightInfo {
	fn set_vanity_name() -> Weight;
	fn spawn_sub_account() -> Weight;
	fn as_sub_account() -> Weight;
}

/// Weights for pallet_cf_account_roles using the Substrate node and recommended hardware.
pub struct PalletWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for PalletWeight<T> {
	/// Storage: `AccountRoles::VanityNames` (r:1 w:1)
	/// Proof: `AccountRoles::VanityNames` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn set_vanity_name() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `546`
		//  Estimated: `2031`
		// Minimum execution time: 14_620_000 picoseconds.
		Weight::from_parts(15_116_000, 2031)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn spawn_sub_account() -> Weight {
		Weight::from_parts(0, 0)
			.saturating_add(T::DbWeight::get().reads(0_u64))
			.saturating_add(T::DbWeight::get().writes(0_u64))
	}

	fn as_sub_account() -> Weight {
		Weight::from_parts(0, 0)
			.saturating_add(T::DbWeight::get().reads(0_u64))
			.saturating_add(T::DbWeight::get().writes(0_u64))
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	/// Storage: `AccountRoles::VanityNames` (r:1 w:1)
	/// Proof: `AccountRoles::VanityNames` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn set_vanity_name() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `546`
		//  Estimated: `2031`
		// Minimum execution time: 14_620_000 picoseconds.
		Weight::from_parts(15_116_000, 2031)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}

	fn spawn_sub_account() -> Weight {
		Weight::from_parts(0, 0)
			.saturating_add(ParityDbWeight::get().reads(0_u64))
			.saturating_add(ParityDbWeight::get().writes(0_u64))
	}

	fn as_sub_account() -> Weight {
		Weight::from_parts(0, 0)
			.saturating_add(ParityDbWeight::get().reads(0_u64))
			.saturating_add(ParityDbWeight::get().writes(0_u64))
	}
}
