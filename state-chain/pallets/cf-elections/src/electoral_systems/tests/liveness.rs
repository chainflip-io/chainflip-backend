use sp_std::collections::btree_set::BTreeSet;

use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::liveness::*,
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
	Liveness<ChainBlockHash, ChainBlockNumber, BlockNumber, MockHook, ValidatorId>;

register_checks! {
	SimpleLiveness {
		only_one_election(_pre, post) {
			let election_ids = post.election_identifiers();
			assert_eq!(election_ids.len(), 1, "Only one election should exist.");
		},
		hook_called_once(_pre, _post) {
			assert_eq!(HOOK_CALLED_COUNT.with(|hook_called| hook_called.get()), 1, "Hook should have been called once so far!");
		},
		hook_called_twice(_pre, _post) {
			assert_eq!(HOOK_CALLED_COUNT.with(|hook_called| hook_called.get()), 2, "Hook should have been called twice so far!");
		},
		hook_not_called(_pre, _post) {
			assert_eq!(
				HOOK_CALLED_COUNT.with(|hook_called| hook_called.get()),
				0,
				"Hook should not have been called!"
			);
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
				Check::<SimpleLiveness>::hook_not_called(),
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
				Check::<SimpleLiveness>::hook_not_called(),
			],
		)
		.test_on_finalize(
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_once(),
			],
		)
		.test_on_finalize(
			// we should have reset to the hook not being called, and there's still just one
			// election
			&(INIT_BLOCK + BLOCKS_BETWEEN_CHECKS + 1, INIT_CHAIN_TRACKING_BLOCK),
			|_| {},
			vec![
				Check::<SimpleLiveness>::only_one_election(),
				Check::<SimpleLiveness>::hook_called_once(),
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
				Check::<SimpleLiveness>::hook_called_once(),
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
				Check::<SimpleLiveness>::hook_called_twice(),
			],
		);
}
