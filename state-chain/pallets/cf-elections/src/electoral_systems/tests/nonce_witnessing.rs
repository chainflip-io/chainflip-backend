use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::nonce_wintessing::*,
};

use crate::vote_storage::nonce::NonceVote;
use cf_primitives::AuthorityCount;

thread_local! {
	pub static HOOK_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct MockHook;
impl OnChangeHook<(), u64> for MockHook {
	fn on_change(_id: (), _value: u64) {
		HOOK_CALLED.with(|hook_called| hook_called.set(true));
	}
}

impl MockHook {
	pub fn called() -> bool {
		HOOK_CALLED.with(|hook_called| hook_called.get())
	}
}

type Vote = NonceVote<Value, Slot>;
type Value = u64;
type Slot = u32;
type SimpleChange = NonceWitnessing<(), Value, Slot, (), MockHook, ()>;

register_checks! {
	SimpleChange {
		hook_called(_pre, _post) {
			assert!(MockHook::called(), "Hook should have been called!");
		},
		hook_not_called(_pre, _post) {
			assert!(
				!MockHook::called(),
				"Hook should not have been called!"
			);
		},
	}
}

const AUTHORITY_COUNT: AuthorityCount = 10;
const THRESHOLD: AuthorityCount = cf_utilities::threshold_from_share_count(AUTHORITY_COUNT);
const SUCCESS_THRESHOLD: AuthorityCount =
	cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);

fn with_default_state() -> TestContext<SimpleChange> {
	TestSetup::<SimpleChange>::default().build_with_initial_election()
}

fn generate_votes(
	correct_voters: AuthorityCount,
	correct_value: NonceVote<Value, Slot>,
	incorrect_voters: AuthorityCount,
	incorrect_value: NonceVote<Value, Slot>,
) -> ConsensusVotes<SimpleChange> {
	ConsensusVotes {
		votes: (0..correct_voters)
			.map(|_| ConsensusVote { vote: Some(((), correct_value.clone())), validator_id: () })
			.chain((0..incorrect_voters).map(|_| ConsensusVote {
				vote: Some(((), incorrect_value.clone())),
				validator_id: (),
			}))
			.chain(
				(0..AUTHORITY_COUNT - correct_voters - incorrect_voters)
					.map(|_| ConsensusVote { vote: None, validator_id: () }),
			)
			.collect(),
	}
}

fn generate_votes_with_differen_slots(
	correct_voters: AuthorityCount,
	correct_value: NonceVote<Value, Slot>,
	incorrect_voters: AuthorityCount,
	incorrect_value: NonceVote<Value, Slot>,
) -> ConsensusVotes<SimpleChange> {
	ConsensusVotes {
		votes: (0..correct_voters)
			.enumerate()
			.map(|(index, _)| ConsensusVote {
				vote: Some(((), NonceVote { value: correct_value.value, slot: index as u32 })),
				validator_id: (),
			})
			.chain((0..incorrect_voters).map(|_| ConsensusVote {
				vote: Some(((), incorrect_value.clone())),
				validator_id: (),
			}))
			.chain(
				(0..AUTHORITY_COUNT - correct_voters - incorrect_voters)
					.map(|_| ConsensusVote { vote: None, validator_id: () }),
			)
			.collect(),
	}
}

#[test]
fn consensus_not_possible_because_of_different_votes() {
	with_default_state().expect_consensus(
		ConsensusVotes {
			votes: (0..AUTHORITY_COUNT)
				.map(|i| ConsensusVote {
					vote: Some(((), NonceVote { value: i as u64, slot: 0u32 })),
					validator_id: (),
				})
				.collect(),
		},
		None,
	);
}

#[test]
fn consensus_when_all_votes_the_same() {
	with_default_state().expect_consensus(
		generate_votes(
			SUCCESS_THRESHOLD,
			NonceVote { value: 1, slot: 1 },
			0,
			NonceVote { value: 0, slot: 0 },
		),
		Some((1, 1)),
	);
}

#[test]
fn consensus_when_all_votes_the_same_but_different_slot() {
	with_default_state().expect_consensus(
		generate_votes_with_differen_slots(
			SUCCESS_THRESHOLD,
			NonceVote { value: 1, slot: 0 },
			0,
			NonceVote { value: 0, slot: 0 },
		),
		Some((1, 5)),
	);
}

#[test]
fn not_enough_votes_for_consensus() {
	with_default_state().expect_consensus(
		generate_votes(
			THRESHOLD,
			NonceVote { value: 1, slot: 1 },
			0,
			NonceVote { value: 0, slot: 0 },
		),
		None,
	);
}

#[test]
fn minority_cannot_prevent_consensus() {
	const CORRECT_VALUE: Vote = NonceVote { value: 1, slot: 1 };
	const INCORRECT_VALUE: Vote = NonceVote { value: 2, slot: 2 };
	with_default_state().expect_consensus(
		generate_votes(
			SUCCESS_THRESHOLD,
			CORRECT_VALUE,
			AUTHORITY_COUNT - SUCCESS_THRESHOLD,
			INCORRECT_VALUE,
		),
		Some((CORRECT_VALUE.value, CORRECT_VALUE.slot)),
	);
}

#[test]
fn finalization_only_on_consensus_change() {
	with_default_state()
		.expect_consensus(
			generate_votes(
				AUTHORITY_COUNT,
				NonceVote { value: 0, slot: 0 },
				0,
				NonceVote { value: 0, slot: 0 },
			),
			Some((0, 0)),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleChange>::hook_not_called(), Check::assert_unchanged()],
		)
		.expect_consensus(
			generate_votes(
				AUTHORITY_COUNT,
				NonceVote { value: 1, slot: 1 },
				0,
				NonceVote { value: 0, slot: 0 },
			),
			Some((1, 1)),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleChange>::hook_called(), Check::all_elections_deleted()],
		);
}
