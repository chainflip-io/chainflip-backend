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

//! Profiles auction resolution against real chain state.
//!
//! Auction resolution (`resolve_auction_iteratively`) is the read-only computation that runs in
//! `on_initialize` at the rotation boundary. It is suspected of consuming significant weight
//! because the cost is driven by the total number of bidders and by the delegation-optimization
//! loop, neither of which is reflected in the weight actually charged (which uses `winners.len()`).
//!
//! This test replays the real bidder/operator/delegator topology at a given block and times each
//! phase of the resolution so we can locate the hotspot. It mutates no storage, so it can be run
//! repeatedly against the same externalities for stable timings.

use super::*;
use std::time::{Duration, Instant};

type Runtime = state_chain_runtime::Runtime;
type Validator = pallet_cf_validator::Pallet<Runtime>;
type KeygenQualification = <Runtime as pallet_cf_validator::Config>::KeygenQualification;

/// Number of timed repetitions (after a warm-up) to average over. State is fully in-memory, so
/// repeated runs measure warm CPU cost.
const REPS: u32 = 20;

fn time_avg(reps: u32, mut f: impl FnMut()) -> Duration {
	f(); // warm-up
	let t = Instant::now();
	for _ in 0..reps {
		f();
	}
	t.elapsed() / reps
}

#[derive(Debug, Default)]
pub struct Test;

impl RuntimeTest for Test {
	fn run(self, block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		ext.execute_with(|| {
			println!("\n=== Auction resolution profile @ {:?} ===", block_hash);

			// Cold-cache measurement: the very first resolution, before any other read warms the
			// trie node cache. This is the closest in-memory proxy for the cost a node pays when
			// these keys are not yet cached (a real DB under contention amplifies it further). Must
			// be the first storage access in this closure.
			let cold_full = {
				let t = Instant::now();
				let _ = Validator::resolve_auction_iteratively(&Default::default());
				t.elapsed()
			};

			// --- Topology (real operators / validators / delegators at this block) ---
			let qualified = Validator::get_qualified_bidders::<KeygenQualification>();
			let (snapshots, independents) =
				Validator::build_delegation_snapshots::<KeygenQualification>(&Default::default());

			let num_operators = snapshots.len();
			let total_validators: usize = snapshots.values().map(|s| s.validators.len()).sum();
			let total_delegators: usize = snapshots.values().map(|s| s.delegators.len()).sum();
			let num_independents = independents.len();

			let mut per_operator: Vec<usize> =
				snapshots.values().map(|s| s.validators.len()).collect();
			per_operator.sort_unstable_by(|a, b| b.cmp(a));

			println!("Topology:");
			println!("  qualified bidders (B):     {}", qualified.len());
			println!("  operators:                 {}", num_operators);
			println!("  managed validators:        {}", total_validators);
			println!("  delegators:                {}", total_delegators);
			println!("  independent validators:    {}", num_independents);
			println!("  validators/operator (desc): {:?}", per_operator);

			// --- Phase timings ---
			// Split build_delegation_snapshots three ways to locate the hotspot:
			//   get_active_bids            -> raw active-bid reads
			//   get_qualified_bidders      -> + the 8-check KeygenQualification filter
			//   build_delegation_snapshots -> + the per-operator and per-delegator storage loops
			let active_bids_avg = time_avg(REPS, || {
				let _ = Validator::get_active_bids();
			});
			let qualified_avg = time_avg(REPS, || {
				let _ = Validator::get_qualified_bidders::<KeygenQualification>();
			});
			let build_avg = time_avg(REPS, || {
				let _ = Validator::build_delegation_snapshots::<KeygenQualification>(
					&Default::default(),
				);
			});

			let full_avg = time_avg(REPS, || {
				let _ = Validator::resolve_auction_iteratively(&Default::default());
			});

			// --- Instrumented loop: reproduces resolve_auction_iteratively's loop verbatim,
			//     counting outer iterations and full re-sorts. ---
			let (iterations, sorts, num_candidates, winners, loop_avg) =
				match Validator::run_initial_auction(&Default::default()) {
					Ok((initial_outcome, resolver, snaps, auction_bids)) => {
						let num_candidates = auction_bids(&snaps).len();

						// Count iterations / sorts once (deterministic).
						let mut iterations = 0usize;
						let mut sorts = 1usize; // the initial resolve inside run_initial_auction
						{
							let mut snaps = snaps.clone();
							let mut current = initial_outcome.clone();
							loop {
								iterations += 1;
								let old = snaps.clone();
								for s in snaps.values_mut() {
									s.maybe_optimize_bid(&current);
								}
								if snaps == old {
									break;
								} else if let Ok(new_outcome) =
									resolver.resolve_auction(auction_bids(&snaps))
								{
									sorts += 1;
									current = new_outcome;
								} else {
									break;
								}
							}
						}

						// Time just the loop (excluding the initial snapshot build + first sort).
						let loop_avg = time_avg(REPS, || {
							let mut snaps = snaps.clone();
							let mut current = initial_outcome.clone();
							loop {
								let old = snaps.clone();
								for s in snaps.values_mut() {
									s.maybe_optimize_bid(&current);
								}
								if snaps == old {
									break;
								} else if let Ok(new_outcome) =
									resolver.resolve_auction(auction_bids(&snaps))
								{
									current = new_outcome;
								} else {
									break;
								}
							}
						});

						// Winners are taken from a final resolution for reporting.
						let winners = Validator::resolve_auction_iteratively(&Default::default())
							.map(|(o, _)| o.winners.len())
							.unwrap_or(0);

						(iterations, sorts, num_candidates, winners, loop_avg)
					},
					Err(e) => {
						println!("run_initial_auction failed: {:?}", e);
						(0, 0, 0, 0, Duration::ZERO)
					},
				};

			println!("\nResolution:");
			println!("  auction candidates:        {}", num_candidates);
			println!("  winners:                   {}", winners);
			println!("  optimization loop iters:   {}", iterations);
			println!("  full sorts performed:      {}", sorts);

			println!("\nTimings (avg over {} warm reps):", REPS);
			println!("  get_active_bids:            {:?}", active_bids_avg);
			println!(
				"  get_qualified_bidders:      {:?}  (+{:?} qualification)",
				qualified_avg,
				qualified_avg.saturating_sub(active_bids_avg),
			);
			println!(
				"  build_delegation_snapshots: {:?}  (+{:?} operator/delegator loops)",
				build_avg,
				build_avg.saturating_sub(qualified_avg),
			);
			println!("  optimization loop:          {:?}", loop_avg);
			println!("  resolve_auction_iteratively: {:?}  <- total (warm)", full_avg);
			println!(
				"\n  resolve_auction_iteratively (COLD first call): {:?}  ({:.1}x warm)",
				cold_full,
				cold_full.as_secs_f64() / full_avg.as_secs_f64(),
			);
			println!("=====================================\n");
		});

		Ok(())
	}
}
