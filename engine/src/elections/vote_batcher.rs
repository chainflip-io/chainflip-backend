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

//! Collects the votes the per-instance [`Voter`](super::Voter) tasks produce each block and
//! submits them as a single `Environment::submit_elections_votes` extrinsic.
//!
//! Each elections instance has its own `Voter`, and each used to submit its own extrinsic - so a
//! validator sent one per instance per block, paying a full signed extrinsic's overhead
//! (signature verification, nonce, fee, base extrinsic weight) every time. The voters already
//! run on the same tick, derived from the same block stream, so their votes arrive within a few
//! milliseconds of each other and are cheap to gather.

use cf_primitives::MILLISECONDS_PER_BLOCK;
use cf_utilities::task_scope::Scope;
use engine_sc_client::extrinsic_api::signed::SignedExtrinsicApi;
use state_chain_runtime::{chainflip::AllElectionInstancesVotes, Runtime};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

/// Hard cap on how long a batch is held after its first votes arrive.
///
/// Bounded on purpose: an instance whose `filter_votes` request is slow or hung must delay only
/// its own votes by a block, not every other chain's. Late votes ride the next batch.
const BATCHING_WINDOW: Duration = Duration::from_millis(MILLISECONDS_PER_BLOCK / 5);

/// How long to wait after the most recent votes before deciding no more are coming.
///
/// Every voter ticks off the same block stream at the same offset, so their votes arrive within
/// a few milliseconds of each other and a short quiet period is enough to catch the whole set.
/// This is what keeps the common case fast: the voters submit half a block in, timed to land in
/// the next block, so holding a batch for the full window would eat into that. A quiet period
/// also adapts to however many instances have something to say, which is not always all of them.
///
/// Derived from the block time, like [`BATCHING_WINDOW`], so the two keep their 8:1 ratio if the
/// block time changes
const QUIET_PERIOD: Duration = Duration::from_millis(MILLISECONDS_PER_BLOCK / 40);

/// Enough room that a voter is never blocked by the batcher; if it ever fills, votes are dropped
/// rather than stalling the voter, and the election is simply voted in again next block.
const CHANNEL_CAPACITY: usize = 64;

/// Handle the per-instance voters use to hand their votes over for batching.
#[derive(Clone)]
pub struct VoteBatcher {
	votes_sender: mpsc::Sender<AllElectionInstancesVotes>,
}

impl VoteBatcher {
	/// Spawn the batching task and return a handle to it.
	pub fn start<StateChainClient: SignedExtrinsicApi + Send + Sync + 'static>(
		scope: &Scope<'_, anyhow::Error>,
		state_chain_client: Arc<StateChainClient>,
	) -> Self {
		let (votes, receiver) = mpsc::channel(CHANNEL_CAPACITY);
		scope.spawn(async move {
			run(receiver, state_chain_client).await;
			Ok(())
		});
		Self { votes_sender: votes }
	}

	/// Hand one instance's votes to the batcher. `instance` names it, for logging only.
	///
	/// Never blocks the caller: if the batcher has fallen far enough behind to fill the channel,
	/// these votes are dropped and the election is voted in again on the next block, which is
	/// preferable to holding up the voter that produced them.
	pub fn send(&self, instance: &'static str, votes: AllElectionInstancesVotes) {
		if let Err(error) = self.votes_sender.try_send(votes) {
			tracing::warn!("Dropping {instance} votes, vote batcher is not keeping up: {error}");
		}
	}
}

/// The batches being gathered, and when they should go out.
///
/// Usually one batch: every instance has its own slot, so a block's votes fit together. A second
/// appears only when an instance sends more votes than one batch can carry - the voters chunk at
/// `MAXIMUM_VOTES_PER_EXTRINSIC` - and then the overflow waits alongside rather than forcing the
/// gathered batch out early, which would strand it in a single-instance extrinsic before the
/// other instances had even reported.
///
/// Separated from the task so the batching decisions can be tested against a clock that is
/// passed in, with no channel or chain client involved.
#[derive(Default)]
struct PendingBatches {
	batches: Vec<AllElectionInstancesVotes>,
	/// When the batches go out regardless, set by the first votes gathered.
	hard_cap: Option<tokio::time::Instant>,
	/// When to send: `QUIET_PERIOD` after the latest votes, but never past `hard_cap`.
	deadline: Option<tokio::time::Instant>,
}

impl PendingBatches {
	/// Add one instance's votes to the first batch with room for them, starting a new batch if
	/// every existing one already carries that instance.
	fn insert(&mut self, votes: AllElectionInstancesVotes, now: tokio::time::Instant) {
		let mut votes = votes;
		for batch in self.batches.iter_mut() {
			match batch.try_merge(votes) {
				Ok(()) => {
					votes = AllElectionInstancesVotes::default();
					break
				},
				// No room in this one; `try_merge` handed the votes back untouched.
				Err(returned) => votes = returned,
			}
		}
		if votes.instances() > 0 {
			self.batches.push(votes);
		}

		let cap = *self.hard_cap.get_or_insert(now + BATCHING_WINDOW);
		self.deadline = Some(cap.min(now + QUIET_PERIOD));
	}

	fn deadline(&self) -> Option<tokio::time::Instant> {
		self.deadline
	}

	/// Take everything gathered and start afresh.
	fn take(&mut self) -> Vec<AllElectionInstancesVotes> {
		self.hard_cap = None;
		self.deadline = None;
		core::mem::take(&mut self.batches)
	}
}

async fn run<StateChainClient: SignedExtrinsicApi + Send + Sync + 'static>(
	mut receiver: mpsc::Receiver<AllElectionInstancesVotes>,
	state_chain_client: Arc<StateChainClient>,
) {
	let mut pending = PendingBatches::default();

	loop {
		let wait = async {
			match pending.deadline() {
				Some(deadline) => tokio::time::sleep_until(deadline).await,
				// Nothing pending, so nothing to wait for - park until votes arrive.
				None => std::future::pending::<()>().await,
			}
		};

		tokio::select! {
			received = receiver.recv() => {
				let Some(votes) = received else {
					// Every voter has gone away; flush anything held so it is not lost.
					submit(pending.take(), &state_chain_client).await;
					break
				};
				pending.insert(votes, tokio::time::Instant::now());
			},
			() = wait => submit(pending.take(), &state_chain_client).await,
		}
	}
}

async fn submit<StateChainClient: SignedExtrinsicApi + Send + Sync + 'static>(
	batches: Vec<AllElectionInstancesVotes>,
	state_chain_client: &Arc<StateChainClient>,
) {
	for batch in batches {
		state_chain_client
			.submit_signed_extrinsic(
				pallet_cf_environment::Call::<Runtime>::submit_elections_votes {
					votes: Box::new(batch),
				},
			)
			.await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::instances::{Instance3, Instance5, Instance7};
	use state_chain_runtime::chainflip::BatchedInstance;
	use std::collections::BTreeMap;

	/// One instance's votes, as its `Voter` would hand them over.
	fn bitcoin_votes() -> AllElectionInstancesVotes {
		<Instance3 as BatchedInstance>::votes(BTreeMap::new().try_into().unwrap())
	}

	fn solana_votes() -> AllElectionInstancesVotes {
		<Instance5 as BatchedInstance>::votes(BTreeMap::new().try_into().unwrap())
	}

	fn tron_votes() -> AllElectionInstancesVotes {
		<Instance7 as BatchedInstance>::votes(BTreeMap::new().try_into().unwrap())
	}

	#[tokio::test]
	async fn votes_wait_a_quiet_period_for_their_neighbours() {
		let start = tokio::time::Instant::now();
		let mut pending = PendingBatches::default();

		pending.insert(bitcoin_votes(), start);
		assert_eq!(pending.deadline(), Some(start + QUIET_PERIOD));

		// A second instance reporting later pushes the send out, so batches go only once the
		// voters have gone quiet - not a fixed time after the first of them.
		let later = start + Duration::from_millis(50);
		pending.insert(solana_votes(), later);
		assert_eq!(pending.deadline(), Some(later + QUIET_PERIOD));

		// Different instances share one batch, so this is still a single extrinsic.
		let batches = pending.take();
		assert_eq!(batches.len(), 1);
		assert_eq!(batches[0].instances(), 2);
	}

	#[tokio::test]
	async fn a_straggler_cannot_hold_the_batch_past_the_window() {
		let start = tokio::time::Instant::now();
		let mut pending = PendingBatches::default();
		pending.insert(bitcoin_votes(), start);

		// Votes arriving just before the window closes must not extend it: the batch is capped
		// from when it started, so one slow instance delays only itself.
		let nearly_up = start + BATCHING_WINDOW - Duration::from_millis(10);
		pending.insert(solana_votes(), nearly_up);
		assert_eq!(pending.deadline(), Some(start + BATCHING_WINDOW));
	}

	#[tokio::test]
	async fn an_instances_overflow_waits_alongside_rather_than_forcing_a_send() {
		let start = tokio::time::Instant::now();
		let mut pending = PendingBatches::default();

		// A voter with more votes than one extrinsic can carry sends them as several chunks,
		// back to back. The overflow must not push the gathered batch out early - that would
		// strand it in a single-instance extrinsic before the other instances had reported.
		pending.insert(bitcoin_votes(), start);
		pending.insert(bitcoin_votes(), start);
		pending.insert(bitcoin_votes(), start);
		// Other instances arrive afterwards and still join the first batch.
		pending.insert(solana_votes(), start);
		pending.insert(tron_votes(), start);

		let batches = pending.take();
		assert_eq!(batches.len(), 3, "one batch per Bitcoin chunk");
		// The other instances rode along with the first chunk rather than trailing behind it.
		assert_eq!(batches[0].instances(), 3);
		assert_eq!(batches[1].instances(), 1);
		assert_eq!(batches[2].instances(), 1);
		assert!(batches.iter().all(|batch| batch.bitcoin.is_some()));
	}

	#[tokio::test]
	async fn nothing_is_pending_until_votes_arrive() {
		let mut pending = PendingBatches::default();
		assert!(pending.take().is_empty());
		assert_eq!(pending.deadline(), None);
	}

	#[tokio::test]
	async fn taking_the_batches_clears_the_deadline() {
		let start = tokio::time::Instant::now();
		let mut pending = PendingBatches::default();
		pending.insert(bitcoin_votes(), start);

		assert_eq!(pending.take().len(), 1);
		// Nothing pending, so the task parks instead of waking on a stale deadline.
		assert_eq!(pending.deadline(), None);
		assert!(pending.take().is_empty());
	}

	#[tokio::test]
	async fn try_merge_hands_back_votes_it_cannot_take() {
		let mut batch = bitcoin_votes();
		assert_eq!(batch.instances(), 1);

		// A free slot is taken...
		assert!(batch.try_merge(solana_votes()).is_ok());
		assert_eq!(batch.instances(), 2);

		// ...and an occupied one hands the votes straight back, so they cannot be lost by
		// being silently overwritten.
		let returned = batch.try_merge(bitcoin_votes()).expect_err("bitcoin slot is taken");
		assert!(returned.bitcoin.is_some());
		assert_eq!(batch.instances(), 2);
		assert!(batch.try_merge(tron_votes()).is_ok());
	}

	#[tokio::test]
	async fn each_instance_fills_only_its_own_slot() {
		// Batches are assembled by merging, so an impl filling more than its own slot would
		// silently overwrite another instance's votes.
		assert_eq!(bitcoin_votes().instances(), 1);
		assert!(bitcoin_votes().bitcoin.is_some());
		assert!(solana_votes().solana.is_some());
		assert!(tron_votes().tron.is_some());
	}
}
