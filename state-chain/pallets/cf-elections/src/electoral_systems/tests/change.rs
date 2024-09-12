use super::{mocks::*, register_checks};
use crate::electoral_systems::change::*;

use cf_primitives::AuthorityCount;

thread_local! {
	pub static HOOK_HAS_BEEN_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct MockHook;
impl OnChangeHook<(), u64> for MockHook {
	fn on_change(_id: (), _value: u64) {
		HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(true));
	}
}

impl MockHook {
	pub fn called() -> bool {
		HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.get())
	}
}

type Vote = u64;
type SimpleChange = Change<(), Vote, (), MockHook>;

register_checks! {
	SimpleChange {
		hook_has_been_called(_pre, _post) {
			assert!(MockHook::called(), "Hook should have been called!");
		},
		hook_not_been_called(_pre, _post) {
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
	TestSetup::<SimpleChange>::default().build()
}

#[test]
fn consensus_not_possible_because_of_different_votes() {
	with_default_state().expect_consensus(
		AUTHORITY_COUNT,
		(0..AUTHORITY_COUNT).map(|i| ((), i as Vote)).collect(),
		None,
	);
}

#[test]
fn consensus_when_all_votes_the_same() {
	with_default_state().expect_consensus(
		AUTHORITY_COUNT,
		vec![((), 1); SUCCESS_THRESHOLD as usize],
		Some(1),
	);
}

#[test]
fn not_enough_votes_for_consensus() {
	with_default_state().expect_consensus(
		AUTHORITY_COUNT,
		(0..THRESHOLD).map(|i| ((), i as Vote)).collect(),
		None,
	);
}

#[test]
fn minority_cannot_prevent_consensus() {
	const CORRECT_VALUE: Vote = 1;
	const INCORRECT_VALUE: Vote = 2;
	with_default_state().expect_consensus(
		AUTHORITY_COUNT,
		(0..SUCCESS_THRESHOLD)
			.map(|_| ((), CORRECT_VALUE))
			.chain((SUCCESS_THRESHOLD..AUTHORITY_COUNT).map(|_| ((), INCORRECT_VALUE)))
			.collect(),
		Some(CORRECT_VALUE),
	);
}

#[test]
fn finalization_only_on_consensus_change() {
	with_default_state()
		.expect_consensus(
			AUTHORITY_COUNT,
			vec![((), Vote::default()); AUTHORITY_COUNT as usize],
			Some(Vote::default()),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleChange>::hook_not_been_called(), Check::assert_unchanged()],
		)
		.expect_consensus(
			AUTHORITY_COUNT,
			vec![((), Vote::default() + 1); AUTHORITY_COUNT as usize],
			Some(Vote::default() + 1),
		)
		.test_on_finalize(
			&(),
			|_| {
				assert!(!MockHook::called());
			},
			vec![Check::<SimpleChange>::hook_has_been_called()],
		);
}
