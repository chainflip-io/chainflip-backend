use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::monotonic_change::*,
};

use crate::vote_storage::change::MonotonicChangeVote;
use cf_primitives::AuthorityCount;
use cf_utilities::assert_panics;

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

type Vote = MonotonicChangeVote<Value, Slot>;
type Value = u64;
type Slot = u32;
type SimpleMonotonicChange = MonotonicChange<(), Value, Slot, (), MockHook, ()>;

register_checks! {
	SimpleMonotonicChange {
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

fn with_default_state() -> TestContext<SimpleMonotonicChange> {
	TestSetup::<SimpleMonotonicChange>::default().build_with_initial_election()
}

fn generate_votes(
	correct_voters: AuthorityCount,
	correct_value: MonotonicChangeVote<Value, Slot>,
	incorrect_voters: AuthorityCount,
	incorrect_value: MonotonicChangeVote<Value, Slot>,
) -> ConsensusVotes<SimpleMonotonicChange> {
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

fn generate_votes_with_different_slots(
	correct_voters: AuthorityCount,
	correct_value: MonotonicChangeVote<Value, Slot>,
	incorrect_voters: AuthorityCount,
	incorrect_value: MonotonicChangeVote<Value, Slot>,
) -> ConsensusVotes<SimpleMonotonicChange> {
	ConsensusVotes {
		votes: (0..correct_voters)
			.enumerate()
			.map(|(index, _)| ConsensusVote {
				vote: Some((
					(),
					MonotonicChangeVote { value: correct_value.value, block: index as u32 },
				)),
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
					vote: Some(((), MonotonicChangeVote { value: i as u64, block: 0u32 })),
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
			MonotonicChangeVote { value: 1, block: 1 },
			0,
			MonotonicChangeVote { value: 0, block: 0 },
		),
		Some((1, 1)),
	);
}

#[test]
fn consensus_when_all_votes_the_same_but_different_blocks() {
	with_default_state().expect_consensus(
		generate_votes_with_different_slots(
			SUCCESS_THRESHOLD,
			MonotonicChangeVote { value: 1, block: 0 },
			3,
			MonotonicChangeVote { value: 0, block: 0 },
		),
		Some((1, 6)),
	);
	with_default_state().expect_consensus(
		generate_votes_with_different_slots(
			AUTHORITY_COUNT,
			MonotonicChangeVote { value: 1, block: 0 },
			0,
			MonotonicChangeVote { value: 0, block: 0 },
		),
		Some((1, 6)),
	);
}

#[test]
fn no_consensus_when_votes_are_filtered_because_invalid() {
	with_default_state()
		.force_consensus_update(crate::electoral_system::ConsensusStatus::Gained {
			most_recent: Some((10, 12)),
			new: (11, 13),
		})
		.expect_consensus(
			generate_votes_with_different_slots(
				0,
				MonotonicChangeVote { value: 0, block: 0 },
				AUTHORITY_COUNT,
				// neither the value has changed nor the block, both of which fail validity checks
				MonotonicChangeVote { value: 11, block: 13 },
			),
			None,
		);
}

#[test]
fn not_enough_votes_for_consensus() {
	with_default_state().expect_consensus(
		generate_votes(
			THRESHOLD,
			MonotonicChangeVote { value: 1, block: 1 },
			0,
			MonotonicChangeVote { value: 0, block: 0 },
		),
		None,
	);
}

#[test]
fn minority_cannot_prevent_consensus() {
	const CORRECT_VALUE: Vote = MonotonicChangeVote { value: 1, block: 1 };
	const INCORRECT_VALUE: Vote = MonotonicChangeVote { value: 2, block: 2 };
	with_default_state().expect_consensus(
		generate_votes(
			SUCCESS_THRESHOLD,
			CORRECT_VALUE,
			AUTHORITY_COUNT - SUCCESS_THRESHOLD,
			INCORRECT_VALUE,
		),
		Some((CORRECT_VALUE.value, CORRECT_VALUE.block)),
	);
}

#[test]
fn finalization_only_on_consensus_change() {
	if cfg!(debug_assertions) {
		assert_panics!(with_default_state()
			.expect_consensus(
				generate_votes(
					AUTHORITY_COUNT,
					MonotonicChangeVote { value: 0, block: 0 },
					0,
					MonotonicChangeVote { value: 0, block: 0 },
				),
				Some((0, 0)),
			)
			.test_on_finalize(
				&(),
				|_| {
					assert!(!MockHook::called());
				},
				vec![Check::<SimpleMonotonicChange>::hook_not_called(), Check::assert_unchanged()],
			));
	}
	with_default_state()
		.expect_consensus(
			generate_votes(
				AUTHORITY_COUNT,
				MonotonicChangeVote { value: 1, block: 1 },
				0,
				MonotonicChangeVote { value: 0, block: 0 },
			),
			Some((1, 1)),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleMonotonicChange>::hook_called(), Check::all_elections_deleted()],
		);
}
