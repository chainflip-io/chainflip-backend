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

//! Submits the votes the per-instance [`Voter`](super::Voter) tasks produce each block, gathering
//! a block's worth into a single `Environment::submit_elections_votes` extrinsic.
//!
//! Each elections instance has its own `Voter`, and each used to submit its own extrinsic - so a
//! validator sent one per instance per block, paying a full signed extrinsic's overhead
//! (signature verification, nonce, fee, base extrinsic weight) every time. The voters already
//! run on the same tick, derived from the same block stream, so their votes arrive within a few
//! milliseconds of each other and are cheap to gather.

use cf_primitives::MILLISECONDS_PER_BLOCK;
use cf_utilities::{task_scope::Scope, UnendingStream};
use engine_sc_client::{
	chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
};
use state_chain_runtime::{
	chainflip::{AllElectionInstancesVotes, BatchedInstance},
	Runtime, RuntimeCall,
};
use std::{
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::mpsc;

/// Enough room that a voter is never blocked by the batching task; if it ever fills, votes are
/// dropped rather than stalling the voter, and the election is simply voted in again next block.
const CHANNEL_CAPACITY: usize = 64;

/// Handle the per-instance voters use to hand their votes over for submission.
pub struct VoteSubmitter<StateChainClient> {
	votes_sender: mpsc::Sender<AllElectionInstancesVotes>,
	batching_enabled: Arc<AtomicBool>,
	state_chain_client: Arc<StateChainClient>,
}

impl<StateChainClient> Clone for VoteSubmitter<StateChainClient> {
	fn clone(&self) -> Self {
		Self {
			votes_sender: self.votes_sender.clone(),
			batching_enabled: self.batching_enabled.clone(),
			state_chain_client: self.state_chain_client.clone(),
		}
	}
}

impl<StateChainClient> VoteSubmitter<StateChainClient> {
	/// Spawn the batching task and return a handle to it.
	pub fn start(
		scope: &Scope<'_, anyhow::Error>,
		state_chain_client: Arc<StateChainClient>,
	) -> Self
	where
		StateChainClient: SignedExtrinsicApi + StorageApi + ChainApi + Send + Sync + 'static,
	{
		let (votes_sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
		let batching_enabled = Arc::new(AtomicBool::new(true));

		scope.spawn({
			let state_chain_client = state_chain_client.clone();
			let batching_enabled = batching_enabled.clone();
			async move {
				watch_election_vote_batching_disabled(state_chain_client, batching_enabled).await;
				Ok(())
			}
		});
		scope.spawn({
			let state_chain_client = state_chain_client.clone();
			async move {
				run(receiver, state_chain_client).await;
				Ok(())
			}
		});

		Self { votes_sender, batching_enabled, state_chain_client }
	}

	/// Submit one instance's votes.
	///
	/// With batching off the votes go out from here as their own per-instance
	/// `pallet_cf_elections::Call::vote`: a single-instance batch would not do, because the point
	/// of the switch is to leave the batched path entirely.
	///
	/// Batched votes never block the caller - if the channel is full they are dropped, and the
	/// election is voted in again next block.
	pub async fn submit<Instance>(
		&self,
		votes: pallet_cf_elections::AuthorityVotes<Runtime, Instance>,
	) where
		Instance: BatchedInstance,
		Runtime: pallet_cf_elections::Config<Instance>,
		pallet_cf_elections::Call<Runtime, Instance>: Into<RuntimeCall>,
		StateChainClient: SignedExtrinsicApi + Send + Sync + 'static,
	{
		if self.batching_enabled.load(Ordering::Relaxed) {
			if let Err(error) = self.votes_sender.try_send(Instance::votes(votes)) {
				tracing::warn!("Dropping votes, vote batching task is not keeping up: {error}");
			}
		} else {
			// TODO: Use block hash you got this vote tasks details from as the based of the mortal
			// of the extrinsic
			self.state_chain_client
				.submit_signed_extrinsic::<RuntimeCall>(
					pallet_cf_elections::Call::<Runtime, Instance>::vote {
						authority_votes: Box::new(votes),
					}
					.into(),
				)
				.await;
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
	/// Present only while batches are gathered.
	timing: Option<BatchTiming>,
}

/// When the batches being gathered should go out.
struct BatchTiming {
	/// When they go out regardless, set by the first votes gathered.
	hard_cap: tokio::time::Instant,
	/// When to send: a quiet period after the latest votes, but never past `hard_cap`.
	deadline: tokio::time::Instant,
}

impl BatchTiming {
	/// Hard cap on how long a batch is held after its first votes arrive.
	///
	/// Bounded on purpose: an instance whose `filter_votes` request is slow or hung must delay
	/// only its own votes by a block, not every other chain's. Late votes ride the next batch.
	const BATCHING_WINDOW_MILLIS: u64 = MILLISECONDS_PER_BLOCK / 5;

	/// How long to wait after the most recent votes before deciding no more are coming.
	///
	/// Every voter ticks off the same block stream at the same offset, so a short quiet period
	/// catches the whole set. Waiting the full window instead would eat into the half block the
	/// voters leave for the extrinsic to land.
	const QUIET_PERIOD_MILLIS: u64 = Self::BATCHING_WINDOW_MILLIS / 8;

	/// Timing for a batch whose first votes arrived at `now`.
	fn with_default_cap(now: tokio::time::Instant) -> Self {
		Self {
			hard_cap: now + Duration::from_millis(Self::BATCHING_WINDOW_MILLIS),
			deadline: now + Duration::from_millis(Self::QUIET_PERIOD_MILLIS),
		}
	}

	/// Move the send to a quiet period after `now`, but no further than the hard cap.
	fn adjust_deadline(&mut self, now: tokio::time::Instant) {
		self.deadline = self.hard_cap.min(now + Duration::from_millis(Self::QUIET_PERIOD_MILLIS));
	}
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

		self.timing
			.get_or_insert_with(|| BatchTiming::with_default_cap(now))
			.adjust_deadline(now);
	}

	fn deadline(&self) -> Option<tokio::time::Instant> {
		self.timing.as_ref().map(|timing| timing.deadline)
	}

	/// Take everything gathered and start afresh.
	fn take(&mut self) -> Vec<AllElectionInstancesVotes> {
		self.timing = None;
		core::mem::take(&mut self.batches)
	}
}

/// Track `Environment::ElectionVoteBatchingDisabled` so the voters can see governance turning
/// batching off without a restart.
///
/// Follows unfinalized blocks rather than finalized ones: this is a switch that exists to be
/// reached for in a hurry, and a reorg flipping it back costs at most a block of votes going out
/// in the other shape, which both paths accept.
async fn watch_election_vote_batching_disabled<
	StateChainClient: StorageApi + ChainApi + Send + Sync + 'static,
>(
	state_chain_client: Arc<StateChainClient>,
	batching_enabled: Arc<AtomicBool>,
) {
	let mut block_stream = state_chain_client.unfinalized_block_stream().await;

	loop {
		let block = block_stream.next_or_pending().await;

		match state_chain_client
			.storage_value::<pallet_cf_environment::ElectionVoteBatchingDisabled<Runtime>>(
				block.hash,
			)
			.await
		{
			Ok(disabled) => batching_enabled.store(!disabled, Ordering::Relaxed),
			Err(error) => {
				// Leave the flag as it was: a failed read says nothing about what governance
				// wants, and guessing either way is worse than carrying on.
				tracing::warn!(
					"Could not read the vote batching switch at block {}, leaving batching {}: {error}",
					block.number,
					if batching_enabled.load(Ordering::Relaxed) { "enabled" } else { "disabled" },
				);
			},
		}
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
		assert_eq!(
			pending.deadline(),
			Some(start + Duration::from_millis(BatchTiming::QUIET_PERIOD_MILLIS))
		);

		// A second instance reporting later pushes the send out, so batches go only once the
		// voters have gone quiet - not a fixed time after the first of them.
		let later = start + Duration::from_millis(50);
		pending.insert(solana_votes(), later);
		assert_eq!(
			pending.deadline(),
			Some(later + Duration::from_millis(BatchTiming::QUIET_PERIOD_MILLIS))
		);

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
		let nearly_up = start + Duration::from_millis(BatchTiming::BATCHING_WINDOW_MILLIS) -
			Duration::from_millis(10);
		pending.insert(solana_votes(), nearly_up);
		assert_eq!(
			pending.deadline(),
			Some(start + Duration::from_millis(BatchTiming::BATCHING_WINDOW_MILLIS))
		);
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
