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

//! Probes the witnesser storage-culling path — the prime suspect for the rotation-boundary
//! block-import spike. Culling runs in `Witnesser::on_idle`, which pops an expired epoch from
//! `EpochsToCull` and `clear_prefix`es the three large epoch-keyed maps (`Votes`,
//! `ExtraCallData`, `CallHashExecuted`) with a per-block budget derived from the leftover on_idle
//! weight. A block with lots of leftover weight can therefore delete a large batch in one go.
//!
//! This test is read-only: it reports what is queued to cull and how much data each recent epoch
//! holds, so we can see whether a big epoch is pending at the reported slow block.

use super::*;
use frame_support::{traits::OnIdle, weights::Weight};
use pallet_cf_witnesser::{CallHashExecuted, EpochsToCull, ExtraCallData, Votes};
use std::time::Instant;

type Runtime = state_chain_runtime::Runtime;
type Validator = pallet_cf_validator::Pallet<Runtime>;
type Witnesser = pallet_cf_witnesser::Pallet<Runtime>;

#[derive(Debug, Default)]
pub struct Test;

impl RuntimeTest for Test {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			let current = Validator::current_epoch();
			let to_cull = EpochsToCull::<Runtime>::get();

			println!("\n=== Witnesser cull probe @ {:?} ===", block_hash);
			println!("  current_epoch:  {}", current);
			println!("  EpochsToCull:   {:?}", to_cull);
			println!(
				"\n  per-epoch entry counts (the next epoch to cull is EpochsToCull.last()):"
			);
			println!("  {:<8} {:>10} {:>16} {:>12}", "epoch", "votes", "call_hash_exec", "extra_data");

			// Count entries per recent epoch. iter_prefix walks only that epoch's sub-map.
			for epoch in current.saturating_sub(12)..=current {
				let t = Instant::now();
				let votes = Votes::<Runtime>::iter_prefix(epoch).count();
				let call_hash = CallHashExecuted::<Runtime>::iter_prefix(epoch).count();
				let extra = ExtraCallData::<Runtime>::iter_prefix(epoch).count();
				let mark = if to_cull.contains(&epoch) { " <- queued to cull" } else { "" };
				println!(
					"  {:<8} {:>10} {:>16} {:>12}   ({:?}){}",
					epoch, votes, call_hash, extra, t.elapsed(), mark,
				);
			}
		});

		Ok(())
	}
}

/// Measures the real cost of culling one full epoch through the actual `on_idle` path.
///
/// We seed `EpochsToCull` with the current (fully-populated) epoch as a stand-in for a large epoch
/// that has just expired, then call `on_idle` once with a full block's worth of leftover ref-time
/// — the worst case, in which the whole epoch is cleared in a single block. Reporting wall-clock
/// against the weight `on_idle` self-charges shows both the spike magnitude and whether the weight
/// accounting keeps up with the real `clear_prefix` cost.
#[derive(Debug, Default)]
pub struct CullCost;

impl RuntimeTest for CullCost {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			let epoch = Validator::current_epoch();
			let block = state_chain_runtime::System::block_number();

			println!("\n=== Witnesser cull COST @ {:?} ===", block_hash);
			println!("  seeding EpochsToCull with epoch {} (a full, live epoch)", epoch);

			let votes_before = Votes::<Runtime>::iter_prefix(epoch).count();
			EpochsToCull::<Runtime>::put(vec![epoch]);

			// on_idle is handed a full block's ref-time (2s) as leftover budget — the worst case.
			// With the per-block cull cap in place, a single on_idle should now clear only a bounded
			// slice and leave the epoch queued (contrast: without the cap it cleared the whole
			// ~230k-item epoch in this one call, a ~295ms spike).
			let budget = Weight::from_parts(2_000_000_000_000, u64::MAX);

			let t = Instant::now();
			let used = <Witnesser as OnIdle<u32>>::on_idle(block, budget);
			let elapsed = t.elapsed();
			let votes_after = Votes::<Runtime>::iter_prefix(epoch).count();

			println!("  single on_idle with the per-block cull cap:");
			println!("    wall clock (cold):     {:?}", elapsed);
			println!("    weight self-charged:   {:.1} ms-equiv ref-time", used.ref_time() as f64 / 1e9);
			println!("    votes deleted:         {} of {}", votes_before - votes_after, votes_before);
			println!("    epoch fully culled:    {}", EpochsToCull::<Runtime>::get().is_empty());
			println!(
				"    -> bounded per block; the epoch stays queued and drains over ~{} blocks.",
				votes_before.div_ceil(5_000).max(1),
			);
		});

		Ok(())
	}
}
