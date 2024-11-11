use super::{
	mocks::{Check, MockAccess, TestContext, TestSetup},
	register_checks,
};
use crate::{
	electoral_system::{ConsensusStatus, ConsensusVote, ConsensusVotes, ElectoralReadAccess},
	electoral_systems::{monotonic_median::*, tests::utils::generate_votes},
};
use cf_primitives::AuthorityCount;

type MonotonicMedianTest = MonotonicMedian<u64, (), MockHook, ()>;

fn with_default_setup() -> TestSetup<MonotonicMedianTest> {
	TestSetup::<_>::default()
}

fn with_default_context() -> TestContext<MonotonicMedianTest> {
	with_default_setup().build_with_initial_election()
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
			assert!(post_finalize.unsynchronised_state >= pre_finalize.unsynchronised_state, "Unsynchronised state can not decrease!");
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
	const AUTHORITY_COUNT: AuthorityCount = 10;
	with_default_context().expect_consensus(
		generate_votes(AUTHORITY_COUNT, AUTHORITY_COUNT),
		Some(3), // lower tercile
	);
}

#[test]
fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
	const AUTHORITY_COUNT: AuthorityCount = 10;
	const SUCCESS_THRESHOLD: AuthorityCount =
		cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);

	with_default_context()
		.expect_consensus(generate_votes(SUCCESS_THRESHOLD, AUTHORITY_COUNT), Some(3));
}

#[test]
fn too_few_votes_consensus_not_possible() {
	const AUTHORITY_COUNT: AuthorityCount = 10;
	const LESS_THAN_SUCCESS_THRESHOLD: AuthorityCount =
		cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT) - 1;

	with_default_context()
		.expect_consensus(generate_votes(LESS_THAN_SUCCESS_THRESHOLD, AUTHORITY_COUNT), None);
}

#[test]
fn finalize_election_with_incremented_state() {
	let test = with_default_context();
	let initial_state = MockAccess::<MonotonicMedianTest>::unsynchronised_state().unwrap();
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
				assert_eq!(pre.unsynchronised_state, initial_state);
				assert_eq!(post.unsynchronised_state, new_unsynchronised_state);
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
			.build_with_initial_election()
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
						assert_eq!(pre.unsynchronised_state, INTITIAL_STATE);
						assert_eq!(post.unsynchronised_state, INTITIAL_STATE);
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

	pub fn generate_votes(
		honest_votes: AuthorityCount,
		dishonest_votes: AuthorityCount,
		authority_count: AuthorityCount,
	) -> ConsensusVotes<MonotonicMedianTest> {
		ConsensusVotes {
			votes: (0..honest_votes)
				.map(|_| ConsensusVote { vote: Some(((), HONEST_VALUE)), validator_id: () })
				.chain(
					(0..dishonest_votes).map(|_| ConsensusVote {
						vote: Some(((), DISHONEST_VALUE)),
						validator_id: (),
					}),
				)
				.chain(
					// didn't vote at all
					(0..(authority_count - honest_votes - dishonest_votes))
						.map(|_| ConsensusVote { vote: None, validator_id: () }),
				)
				.collect(),
		}
	}

	// We reach consensus after SUCCESS_THRESHOLD votes.
	// Assumption: everyone votes (not voting is equivalent to dishonest voting).
	//  - The above is a key assumption. If dishonest nodes are capable of preventing honest nodes
	//    from voting, or or preventing them from voting in time, then the 'no-fast-forward'
	//    property is not guaranteed.

	// A superminority can prevent consensus value from advancing.
	with_default_context().expect_consensus(
		generate_votes(AUTHORITY_COUNT - THRESHOLD, THRESHOLD, AUTHORITY_COUNT),
		Some(HONEST_VALUE),
	);

	// A supermajority is required to advance the consensus value.
	with_default_context().expect_consensus(
		generate_votes(AUTHORITY_COUNT - SUCCESS_THRESHOLD, SUCCESS_THRESHOLD, AUTHORITY_COUNT),
		Some(DISHONEST_VALUE),
	);

	// Demonstration that incomplete votes break the assumption.
	// Here, we advance the state despite *not* having a dishonest supermajority.
	with_default_context()
		.expect_consensus(generate_votes(1, THRESHOLD, AUTHORITY_COUNT), Some(DISHONEST_VALUE));
}
