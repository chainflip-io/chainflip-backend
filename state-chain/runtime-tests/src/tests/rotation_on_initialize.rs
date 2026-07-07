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

//! Measures the whole validator-pallet `on_initialize` at the rotation-trigger block, to see how
//! much of it is the auction resolution vs. the rest (keygen kickoff, snapshot-registration writes,
//! missed-authorship punishment).
//!
//! This only produces a measurement when the loaded state is pre-rotation (`Idle` with the epoch
//! expired), because `on_initialize` triggers the auction exactly once and then mutates the phase.
//! If the loaded block already started the rotation, replay against the PARENT block instead.

use super::*;
use frame_support::traits::OnInitialize;
use std::{
	panic::{catch_unwind, AssertUnwindSafe},
	time::Instant,
};

type Runtime = state_chain_runtime::Runtime;
type Validator = pallet_cf_validator::Pallet<Runtime>;

#[derive(Debug, Default)]
pub struct Test;

impl RuntimeTest for Test {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			println!("\n=== Rotation on_initialize profile @ {:?} ===", block_hash);

			let now = state_chain_runtime::System::block_number();
			let started = Validator::current_epoch_started_at();
			let duration = Validator::epoch_duration();
			let trigger = started.saturating_add(duration);
			let phase = Validator::current_rotation_phase();

			println!("  frame_system block number: {}", now);
			println!("  current epoch:             {}", Validator::current_epoch());
			println!("  epoch started at:          {}", started);
			println!("  epoch duration:            {}", duration);
			println!("  rotation trigger block:    {}", trigger);
			println!("  current rotation phase:    {:?}", phase);

			if !matches!(phase, pallet_cf_validator::RotationPhase::Idle) {
				println!(
					"\n  Phase is not Idle -> rotation already started at this block. Replay the \
					 PARENT block to measure the trigger on_initialize."
				);
				return;
			}

			// on_initialize is triggered once (Idle -> KeygensInProgress) and then mutates the
			// phase, so this is a single cold sample. catch_unwind so a decode/logic panic against
			// real state is reported rather than aborting the harness.
			match catch_unwind(AssertUnwindSafe(|| {
				let t = Instant::now();
				let weight = <Validator as OnInitialize<u32>>::on_initialize(trigger);
				(t.elapsed(), weight)
			})) {
				Ok((elapsed, weight)) => {
					println!("\n  Validator::on_initialize(trigger) [single cold sample]:");
					println!("    wall clock:      {:?}", elapsed);
					println!("    returned weight: {} ref-time ps", weight.ref_time());
					println!("    phase after:     {:?}", Validator::current_rotation_phase());
					println!(
						"\n  Compare against the ~5 ms auction-resolution cost from the \
						 auction_resolution test: the difference is keygen kickoff + snapshot \
						 registration writes + missed-authorship punishment."
					);
				},
				Err(_) => {
					println!("\n  Validator::on_initialize PANICKED against real state (see stderr).");
				},
			}
		});

		Ok(())
	}
}
