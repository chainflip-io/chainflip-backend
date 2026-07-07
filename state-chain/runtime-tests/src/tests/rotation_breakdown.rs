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

//! Attributes the rotation-boundary `on_initialize` cost across pallets, to see whether auction
//! resolution (the validator pallet, ~5.6 ms) is the bulk of the block or dwarfed by other work
//! (Witnesser has the largest state; Elections runs consensus every block).
//!
//! Run against the PARENT of the rotation-trigger block (pre-rotation `Idle` state), so
//! `on_initialize` triggers the real rotation. Each measurement is a single cold sample and is
//! `catch_unwind`-guarded.
//!
//! CAVEAT: this branch (BSC integration) changed the Elections storage encoding and added new
//! Elections instances (Tron, Bsc) absent from mainnet state, so the *Elections* numbers here are
//! indicative only — a faithful Elections measurement needs the runtime version mainnet was
//! actually running. Witnesser / Reputation / ChainTracking are unaffected and reliable.

use super::*;
use frame_support::traits::OnInitialize;
use std::{
	panic::{catch_unwind, AssertUnwindSafe},
	time::{Duration, Instant},
};

type Runtime = state_chain_runtime::Runtime;
type Validator = pallet_cf_validator::Pallet<Runtime>;

/// Returns `(trigger_block, is_idle)` after printing the epoch/phase context, or `None` if the
/// state is not pre-rotation (in which case the caller should bail).
fn rotation_context() -> Option<u32> {
	let started = Validator::current_epoch_started_at();
	let duration = Validator::epoch_duration();
	let trigger = started.saturating_add(duration);
	let phase = Validator::current_rotation_phase();
	println!("  epoch {} started at {}, duration {} -> trigger block {}", Validator::current_epoch(), started, duration, trigger);
	println!("  rotation phase: {}", if matches!(phase, pallet_cf_validator::RotationPhase::Idle) { "Idle" } else { "NOT Idle" });
	if matches!(phase, pallet_cf_validator::RotationPhase::Idle) {
		Some(trigger)
	} else {
		println!("  Not pre-rotation: replay the PARENT of the trigger block (N-2).");
		None
	}
}

fn measure<P: OnInitialize<u32>>(name: &str, trigger: u32) -> Duration {
	match catch_unwind(AssertUnwindSafe(|| {
		let t = Instant::now();
		let weight = P::on_initialize(trigger);
		(t.elapsed(), weight)
	})) {
		Ok((elapsed, weight)) => {
			println!("  {:<26} {:>12?}   ({} ref-time ps)", name, elapsed, weight.ref_time());
			elapsed
		},
		Err(_) => {
			println!("  {:<26} PANICKED (branch/mainnet state skew)", name);
			Duration::ZERO
		},
	}
}

/// Times the whole runtime's `on_initialize` in one shot — the total mandatory block work.
#[derive(Debug, Default)]
pub struct Full;

impl RuntimeTest for Full {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			println!("\n=== Full runtime on_initialize @ {:?} ===", block_hash);
			let Some(trigger) = rotation_context() else { return };
			measure::<state_chain_runtime::AllPalletsWithSystem>("AllPalletsWithSystem", trigger);
		});
		Ok(())
	}
}

/// Times individual heavy pallets' `on_initialize` for attribution. Each runs on the same ext in
/// sequence; since they read mostly disjoint storage this gives a fair per-pallet cost, though the
/// sum is only an approximation of the true total (see `Full`).
#[derive(Debug, Default)]
pub struct PerPallet;

impl RuntimeTest for PerPallet {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			use state_chain_runtime::{
				ArbitrumElections, BitcoinElections, EthereumElections, GenericElections,
				Reputation, SolanaElections, Witnesser,
			};
			println!("\n=== Per-pallet on_initialize breakdown @ {:?} ===", block_hash);
			let Some(trigger) = rotation_context() else { return };
			println!();

			let mut total = Duration::ZERO;
			// Validator is measured first because it triggers the rotation (matches real order).
			total += measure::<Validator>("Validator (auction)", trigger);
			total += measure::<Witnesser>("Witnesser", trigger);
			total += measure::<Reputation>("Reputation", trigger);
			total += measure::<GenericElections>("GenericElections*", trigger);
			total += measure::<EthereumElections>("EthereumElections*", trigger);
			total += measure::<BitcoinElections>("BitcoinElections*", trigger);
			total += measure::<SolanaElections>("SolanaElections*", trigger);
			total += measure::<ArbitrumElections>("ArbitrumElections*", trigger);

			println!("\n  sum of measured pallets:    {:?}", total);
			println!("  (* Elections numbers are indicative only - see file header caveat)");
		});
		Ok(())
	}
}
