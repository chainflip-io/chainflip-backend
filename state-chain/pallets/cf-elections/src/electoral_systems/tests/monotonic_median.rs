use super::{
	mocks::{Check, TestContext, TestSetup},
	register_checks,
};
use crate::{
	electoral_system::{ConsensusStatus, ElectoralReadAccess},
	electoral_systems::monotonic_median::*,
};
use cf_primitives::AuthorityCount;

type MonotonicMedianTest = MonotonicMedian<u64, (), MockHook, ()>;

fn with_default_setup() -> TestSetup<MonotonicMedianTest> {
	TestSetup::<_>::default()
}

fn with_default_context() -> TestContext<MonotonicMedianTest> {
	with_default_setup().build()
}

pub struct MockHook;

thread_local! {
	pub static HOOK_HAS_BEEN_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl<T> MedianChangeHook<T> for MockHook {
	fn on_change(_value: T) {
		HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(true));
	}
}

impl MockHook {
	pub fn has_been_called() -> bool {
		HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.get())
	}

	pub fn reset() {
		HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(false));
	}
}

register_checks! {
	MonotonicMedianTest {
		monotonically_increasing_state(pre_finalize, post_finalize) {
			assert!(post_finalize.unsynchronised_state().unwrap() >= pre_finalize.unsynchronised_state().unwrap(),
				"Unsynchronised state can not decrease!");
		},
		hook_called(_pre, _post) {
			assert!(MockHook::has_been_called(), "Hook should have been called!");
		},
		hook_not_called(_pre, _post) {
			assert!(
				!MockHook::has_been_called(),
				"Hook should not have been called!"
			);
		},
	}
}

#[test]
fn check_consensus_correctly_calculates_median_when_all_authorities_vote() {
	const AUTHORITIES: AuthorityCount = 10;
	with_default_context().expect_consensus(
		AUTHORITIES,
		(0..AUTHORITIES).map(|v| ((), v as u64, ())).collect::<Vec<_>>(),
		Some(3), // lower tercile
	);
}

#[test]
fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
	const AUTHORITY_COUNT: AuthorityCount = 10;
	let vote_count = cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);
	let votes = (0..vote_count).map(|v| ((), v as u64, ())).collect::<Vec<_>>();

	with_default_context().expect_consensus(AUTHORITY_COUNT, votes, Some(3));
}

#[test]
fn too_few_votes_consensus_not_possible() {
	const AUTHORITY_COUNT: AuthorityCount = 10;
	let vote_count = cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT) - 1;
	let votes = (0..vote_count).map(|v| ((), v as u64, ())).collect::<Vec<_>>();

	with_default_context().expect_consensus(AUTHORITY_COUNT, votes, None);
}

#[test]
fn finalize_election_with_incremented_state() {
	let test = with_default_context();
	let initial_state = test.access().unsynchronised_state().unwrap();
	let new_unsynchronised_state = initial_state + 1;

	test.force_consensus_update(ConsensusStatus::Gained {
		most_recent: None,
		new: new_unsynchronised_state,
	})
	.test_on_finalize(
		&(),
		|_| {
			assert!(
				!MockHook::has_been_called(),
				"Hook should not have been called before finalization!"
			);
		},
		vec![
			Check::monotonically_increasing_state(),
			Check::<MonotonicMedianTest>::hook_called(),
			Check::new(move |pre, post| {
				assert_eq!(pre.unsynchronised_state().unwrap(), initial_state);
				assert_eq!(post.unsynchronised_state().unwrap(), new_unsynchronised_state);
			}),
			Check::last_election_deleted(),
			Check::election_id_incremented(),
		],
	);
}

#[test]
fn finalize_election_state_can_not_decrease() {
	const INTITIAL_STATE: u64 = 2;

	#[track_caller]
	fn assert_no_update(new_state: u64) {
		assert!(
			new_state <= INTITIAL_STATE,
			"This test is not valid if the new state is higher than the old."
		);
		MockHook::reset();
		with_default_setup()
			.with_unsynchronised_state(INTITIAL_STATE)
			.build()
			// It's possible for authorities to come to consensus on a lower state,
			// but this should not change the unsynchronised state.
			.force_consensus_update(ConsensusStatus::Gained { most_recent: None, new: new_state })
			.test_on_finalize(
				&(),
				|_| {
					assert!(
						!MockHook::has_been_called(),
						"Hook should not have been called before finalization!"
					);
				},
				vec![
					Check::monotonically_increasing_state(),
					// The hook should not be called if the state is not updated.
					Check::<MonotonicMedianTest>::hook_not_called(),
					Check::new(|pre, post| {
						assert_eq!(pre.unsynchronised_state().unwrap(), INTITIAL_STATE);
						assert_eq!(post.unsynchronised_state().unwrap(), INTITIAL_STATE);
					}),
					Check::last_election_deleted(),
					Check::election_id_incremented(),
				],
			);
	}

	// Lower state than the initial state should be invalid.
	assert_no_update(INTITIAL_STATE - 1);
	// Equal state to the initial state should be invalid.
	assert_no_update(INTITIAL_STATE);
}

#[test]
fn minority_can_not_influence_consensus() {
	// Two ways of thinking about this:
	// - A superminority can prevent consensus value from advancing.
	// - A supermajority is required to advance the consensus value.
	//
	// This is why use the lower 33rd percentile vote. If we used the median, a simple majority
	// could influence the consensus value.

	// Assumption: the dishonest value is *higher* than the honest value. (Dishonest nodes are
	// trying to 'speed up' the advancement.)
	const HONEST_VALUE: u64 = 5;
	const DISHONEST_VALUE: u64 = 10;

	const AUTHORITY_COUNT: AuthorityCount = 10;
	const THRESHOLD: AuthorityCount = cf_utilities::threshold_from_share_count(AUTHORITY_COUNT);
	const SUCCESS_THRESHOLD: AuthorityCount =
		cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);

	// We reach consensus after SUCCESS_THRESHOLD votes.
	// Assumption: everyone votes (not voting is equivalent to dishonest voting).
	//  - The above is a key assumption. If dishonest nodes are capable of preventing honest nodes
	//    from voting, or or preventing them from voting in time, then the 'no-fast-forward'
	//    property is not guaranteed.

	// A superminority can prevent consensus value from advancing.
	let dishonest_votes = (0..THRESHOLD).map(|_| ((), DISHONEST_VALUE, ()));
	let consent_votes = (0..(AUTHORITY_COUNT - THRESHOLD)).map(|_| ((), HONEST_VALUE, ()));
	let all_votes = dishonest_votes.chain(consent_votes).collect::<Vec<_>>();
	with_default_context().expect_consensus(AUTHORITY_COUNT, all_votes, Some(HONEST_VALUE));

	// A supermajority is required to advance the consensus value.
	let dishonest_votes = (0..SUCCESS_THRESHOLD).map(|_| ((), DISHONEST_VALUE, ()));
	let consent_votes = (0..(AUTHORITY_COUNT - SUCCESS_THRESHOLD)).map(|_| ((), HONEST_VALUE, ()));
	let all_votes = dishonest_votes.chain(consent_votes).collect::<Vec<_>>();
	with_default_context().expect_consensus(AUTHORITY_COUNT, all_votes, Some(DISHONEST_VALUE));

	// Demonstration that incomplete votes break the assumption.
	// Here, we advance the state despite *not* having a dishonest supermajority.
	let dishonest_votes = (0..THRESHOLD).map(|_| ((), DISHONEST_VALUE, ()));
	let all_votes = dishonest_votes.chain(core::iter::once(((), HONEST_VALUE, ()))).collect();
	with_default_context().expect_consensus(AUTHORITY_COUNT, all_votes, Some(DISHONEST_VALUE));
}
