//! Autogenerated weights for pallet_session
//!
//! THIS FILE WAS AUTO-GENERATED USING CHAINFLIP NODE BENCHMARK CMD VERSION 4.0.0-dev
//! DATE: 2022-06-24, STEPS: `20`, REPEAT: 10, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: None, DB CACHE: 1024

// Executed Command:
// ./target/release/chainflip-node
// benchmark
// pallet
// --extrinsic
// *
// --pallet
// pallet_session
// --output
// ./state-chain/runtime/src/weights/pallet_session.rs
// --execution=wasm
// --steps=20
// --repeat=10
// --template=./state-chain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use sp_std::marker::PhantomData;

use pallet_session::weights::WeightInfo;

/// Weights for pallet_session using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	// Storage: Session NextKeys (r:1 w:1)
	// Storage: Session KeyOwner (r:2 w:4)
	fn set_keys() -> Weight {
		#[allow(clippy::unnecessary_cast)]
		(Weight::from_parts(29_000_000, 0)
)
			.saturating_add(T::DbWeight::get().reads(3u64))
			.saturating_add(T::DbWeight::get().writes(5u64))
	}
	// Storage: Session NextKeys (r:1 w:1)
	// Storage: Session KeyOwner (r:0 w:2)
	fn purge_keys() -> Weight {
		#[allow(clippy::unnecessary_cast)]
		(Weight::from_parts(24_000_000, 0)
)
			.saturating_add(T::DbWeight::get().reads(1u64))
			.saturating_add(T::DbWeight::get().writes(3u64))
	}
}
