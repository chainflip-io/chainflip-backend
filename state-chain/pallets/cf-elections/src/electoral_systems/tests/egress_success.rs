use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::egress_success::*,
};

use cf_primitives::AuthorityCount;

thread_local! {
	pub static HOOK_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct MockHook;
impl OnEgressSuccess<(), EgressData> for MockHook {
	fn on_egress_success(_id: (), _egress_data: EgressData) {
		HOOK_CALLED.with(|hook_called| hook_called.set(true));
	}
}

impl MockHook {
	pub fn called() -> bool {
		HOOK_CALLED.with(|hook_called| hook_called.get())
	}
}

type EgressData = u64;
type SimpleEgressSuccess = EgressSuccess<(), EgressData, (), MockHook, ()>;

register_checks! {
	SimpleEgressSuccess {
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

fn with_default_state() -> TestContext<SimpleEgressSuccess> {
	TestSetup::<SimpleEgressSuccess>::default().build_with_initial_election()
}

fn generate_votes(
	correct_voters: AuthorityCount,
	correct_value: u64,
	incorrect_voters: AuthorityCount,
	incorrect_value: u64,
) -> ConsensusVotes<SimpleEgressSuccess> {
	ConsensusVotes {
		votes: (0..correct_voters)
			.map(|_| ConsensusVote {
				vote: Some(((), correct_value as EgressData)),
				validator_id: (),
			})
			.chain((0..incorrect_voters).map(|_| ConsensusVote {
				vote: Some(((), incorrect_value as EgressData)),
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
				.map(|i| ConsensusVote { vote: Some(((), i as EgressData)), validator_id: () })
				.collect(),
		},
		None,
	);
}

#[test]
fn consensus_when_all_votes_the_same() {
	with_default_state().expect_consensus(generate_votes(SUCCESS_THRESHOLD, 1, 0, 0), Some(1));
}

#[test]
fn not_enough_votes_for_consensus() {
	with_default_state().expect_consensus(generate_votes(THRESHOLD, 1, 0, 0), None);
}

#[test]
fn minority_cannot_prevent_consensus() {
	const CORRECT_VALUE: EgressData = 1;
	const INCORRECT_VALUE: EgressData = 2;
	with_default_state().expect_consensus(
		generate_votes(
			SUCCESS_THRESHOLD,
			CORRECT_VALUE,
			AUTHORITY_COUNT - SUCCESS_THRESHOLD,
			INCORRECT_VALUE,
		),
		Some(CORRECT_VALUE),
	);
}

#[test]
fn finalization_after_vote_success_calls_hook_deletes_election() {
	with_default_state()
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleEgressSuccess>::hook_not_called(), Check::assert_unchanged()],
		)
		.expect_consensus(generate_votes(SUCCESS_THRESHOLD - 1, EgressData::default(), 0, 0), None)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleEgressSuccess>::hook_not_called(), Check::assert_unchanged()],
		)
		.expect_consensus(
			generate_votes(AUTHORITY_COUNT, EgressData::default(), 0, 0),
			Some(EgressData::default()),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleEgressSuccess>::hook_called(), Check::last_election_deleted()],
		);
}
