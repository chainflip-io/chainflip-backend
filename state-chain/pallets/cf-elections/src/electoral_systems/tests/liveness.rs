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

use sp_std::collections::btree_set::BTreeSet;

use super::mocks::*;
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::liveness::*,
	register_checks,
};

pub type ChainBlockHash = u64;
pub type ChainBlockNumber = u64;
pub type BlockNumber = u32;
pub type ValidatorId = u16;

thread_local! {
	pub static HOOK_CALLED_COUNT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

struct MockHook;

impl OnCheckComplete<ValidatorId> for MockHook {
	fn on_check_complete(_validator_ids: BTreeSet<ValidatorId>) {
		HOOK_CALLED_COUNT.with(|hook_called| hook_called.set(hook_called.get() + 1));
	}
}

impl MockHook {
	pub fn called() -> u8 {
		HOOK_CALLED_COUNT.with(|hook_called| hook_called.get())
	}
}

type SimpleLiveness =
	Liveness<ChainBlockHash, ChainBlockNumber, BlockNumber, MockHook, ValidatorId, u32>;

register_checks! {
	SimpleLiveness {
		only_one_election(_pre, post) {
			assert_eq!(post.election_identifiers.len(), 1, "Only one election should exist.");
		},
		hook_called_n_times(_pre, _post, n: u8) {
			assert_eq!(HOOK_CALLED_COUNT.with(|hook_called| hook_called.get()), n, "Hook should have been called {n} times so far!");
		},
	}
}

const CORRECT_VOTE: ChainBlockHash = 69420;
const INCORRECT_VOTE: ChainBlockHash = 666;

fn generate_votes(
	correct_voters: BTreeSet<ValidatorId>,
	incorrect_voters: BTreeSet<ValidatorId>,
	did_not_vote: BTreeSet<ValidatorId>,
) -> ConsensusVotes<SimpleLiveness> {
	ConsensusVotes {
		votes: correct_voters
			.into_iter()
			.map(|v| ConsensusVote { vote: Some(((), CORRECT_VOTE)), validator_id: v })
			.chain(
				incorrect_voters
					.into_iter()
					.map(|v| ConsensusVote { vote: Some(((), INCORRECT_VOTE)), validator_id: v }),
			)
			.chain(did_not_vote.into_iter().map(|v| ConsensusVote { vote: None, validator_id: v }))
			.collect(),
	}
}

fn with_default_state() -> TestContext<SimpleLiveness> {
	TestSetup::<SimpleLiveness>::default().build_with_initial_election()
}

#[test]
fn all_vote_for_same_value_means_no_bad_validators() {
	with_default_state().expect_consensus(
		generate_votes((0..50).collect(), BTreeSet::default(), BTreeSet::default()),
		Some(BTreeSet::default()),
	);
}

#[test]
fn no_votes_no_one_bad_validators() {
	with_default_state().expect_consensus(
		generate_votes(BTreeSet::default(), BTreeSet::default(), (0..50).collect()),
		None,
	);
}

#[test]
fn consensus_with_bad_voters() {
	let correct_voters: BTreeSet<_> = (0..50).collect();
	let incorrect_voters: BTreeSet<_> = (50..60).collect();
	let non_voters: BTreeSet<_> = (60..70).collect();

	with_default_state().expect_consensus(
		generate_votes(correct_voters.clone(), incorrect_voters.clone(), BTreeSet::default()),
		Some(incorrect_voters.clone()),
	);

	with_default_state().expect_consensus(
		generate_votes(correct_voters.clone(), BTreeSet::default(), non_voters.clone()),
		Some(non_voters.clone()),
	);

	with_default_state().expect_consensus(
		generate_votes(correct_voters, incorrect_voters.clone(), non_voters.clone()),
		Some(incorrect_voters.into_iter().chain(non_voters).collect()),
	);
}

#[test]
fn on_finalize() {
	const INIT_BLOCK: BlockNumber = 100;
	const BLOCKS_BETWEEN_CHECKS: BlockNumber = 10;
	const INIT_CHAIN_TRACKING_BLOCK: ChainBlockNumber = 1000;

	let correct_voters: BTreeSet<_> = (0..50).collect();
	let non_voters: BTreeSet<_> = (60..70).collect();

	TestSetup::default()
		.with_electoral_settings(BLOCKS_BETWEEN_CHECKS)
		.build()
		.test_on_finalize(
			&(INIT_BLOCK, INIT_CHAIN_TRACKING_BLOCK),
			|_| assert_eq!(MockHook::called(), 0, "Hook should not have been called!"),
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(0),
			],
		)
		.expect_consensus(
			generate_votes(correct_voters.clone(), BTreeSet::default(), non_voters.clone()),
			Some(non_voters.clone()),
		)
		.test_on_finalize(
			// check duration has not yet elapsed, so no change
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS - 1, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(0),
			],
		)
		.test_on_finalize(
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(1),
			],
		)
		.test_on_finalize(
			// we should have reset to the hook not being called, and there's still just one
			// election
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS + 1, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(1),
			],
		)
		// there have been no votes, so still only called once
		.test_on_finalize(
			// we should have reset to the hook not being called, and there's still just one
			// election
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS * 2, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(1),
			],
		)
		.expect_consensus(
			generate_votes(correct_voters.clone(), BTreeSet::default(), non_voters.clone()),
			Some(non_voters),
		)
		// we have votes now and expect nodes to be punished by having the hook called again.
		.test_on_finalize(
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS * 3, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_n_times(2),
			],
		);
}
