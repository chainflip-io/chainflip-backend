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

//! Autogenerated weights for pallet_timestamp
//!
//! THIS FILE WAS AUTO-GENERATED USING CHAINFLIP NODE BENCHMARK CMD VERSION 4.0.0-dev
//! DATE: 2022-06-13, STEPS: `20`, REPEAT: 10, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: None, DB CACHE: 1024

// Executed Command:
// ./target/release/chainflip-node
// benchmark
// pallet
// --extrinsic
// *
// --pallet
// pallet_timestamp
// --output
// ./state-chain/runtime/src/weights/pallet_timestamp.rs
// --execution=wasm
// --steps=20
// --repeat=10
// --template=./state-chain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use sp_std::marker::PhantomData;

use pallet_timestamp::weights::WeightInfo;

/// Weights for pallet_timestamp using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: Timestamp Now (r:1 w:1)
	// Storage: Aura CurrentSlot (r:1 w:0)
	fn set() -> Weight {
		#[allow(clippy::unnecessary_cast)]
		(Weight::from_parts(11_000_000, 0)
)
			.saturating_add(T::DbWeight::get().reads(2u64))
			.saturating_add(T::DbWeight::get().writes(1u64))
	}
	fn on_finalize() -> Weight {
		#[allow(clippy::unnecessary_cast)]
		(Weight::from_parts(5_000_000, 0)
)
	}
}
