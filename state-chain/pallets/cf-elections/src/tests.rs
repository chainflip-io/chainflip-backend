#![cfg(test)]
use std::collections::BTreeMap;

use crate::{mock::*, *};
use cf_primitives::AuthorityCount;
use electoral_system::{
	AuthorityVoteOf, ConsensusStatus, ElectionReadAccess, ElectoralReadAccess, ElectoralWriteAccess,
};
use electoral_systems::mock::MockElectoralSystem;
use frame_support::traits::OriginTrait;
use vote_storage::AuthorityVote;

pub trait ElectoralSystemTestExt: Sized {
	fn assume_consensus(self) -> Self;
	fn assume_no_consensus(self) -> Self;
	fn expect_consensus(self, expected: ConsensusStatus<AuthorityCount>) -> Self;
	fn new_election(self) -> Self;
	fn submit_simple<I: 'static>(
		self,
		validator_ids: &[u64],
		call: impl Fn(&u64) -> Call<Test, I>,
	) -> Self
	where
		Test: Config<I>;
	fn submit_votes<I: 'static>(
		self,
		validator_ids: &[u64],
		vote: AuthorityVoteOf<MockElectoralSystem>,
		expected_error: Option<Error<Test, I>>,
	) -> Self
	where
		Test: Config<I, ElectoralSystem = MockElectoralSystem>;
}

impl ElectoralSystemTestExt for TestRunner<TestContext> {
	/// Starts a new election, adding its unique monotonic identifier to the test context.
	#[track_caller]
	fn new_election(self) -> Self {
		self.then_execute_with(
			#[track_caller]
			|mut ctx| {
				let unique_monotonic_identifier =
					*Pallet::<Test, Instance1>::with_electoral_access(|electoral_access| {
						electoral_access.new_election((), (), ())
					})
					.expect("New election should not corrupt storage.")
					.election_identifier()
					.unique_monotonic();

				assert_eq!(Status::<Test, Instance1>::get(), Some(ElectoralSystemStatus::Running));

				Pallet::<Test, Instance1>::with_electoral_access(|electoral_access| {
					electoral_access
						.election(ElectionIdentifier::new(unique_monotonic_identifier, ()))
				})
				.expect("Expected an initial election.");

				ctx.umis.push(unique_monotonic_identifier);

				ctx
			},
		)
	}

	#[track_caller]
	fn assume_consensus(self) -> Self {
		MockElectoralSystem::set_assume_consensus(true);
		self
	}

	#[track_caller]
	fn assume_no_consensus(self) -> Self {
		MockElectoralSystem::set_assume_consensus(false);
		self
	}

	#[track_caller]
	fn submit_votes<I: 'static>(
		self,
		validator_ids: &[u64],
		vote: AuthorityVoteOf<MockElectoralSystem>,
		expected_error: Option<Error<Test, I>>,
	) -> Self
	where
		Test: Config<I, ElectoralSystem = MockElectoralSystem>,
	{
		self.then_apply_extrinsics(
			#[track_caller]
			|TestContext { umis, .. }| {
				validator_ids
					.iter()
					.flat_map(|id| {
						umis.iter().map(|umi| {
							(
								OriginTrait::signed(*id),
								Call::<Test, I>::vote {
									authority_votes: BoundedBTreeMap::try_from(
										sp_std::iter::once((
											ElectionIdentifier::new(*umi, ()),
											vote.clone(),
										))
										.collect::<BTreeMap<_, _>>(),
									)
									.unwrap(),
								},
								expected_error.clone().map(|e| Err(e.into())).unwrap_or(Ok(())),
							)
						})
					})
					.collect::<Vec<_>>()
			},
		)
	}

	#[track_caller]
	fn submit_simple<I: 'static>(
		self,
		validator_ids: &[u64],
		call: impl Fn(&u64) -> Call<Test, I>,
	) -> Self
	where
		Test: Config<I>,
	{
		self.then_apply_extrinsics(
			#[track_caller]
			|_ctx| validator_ids.iter().map(|id| (OriginTrait::signed(*id), call(id), Ok(()))),
		)
	}

	#[track_caller]
	fn expect_consensus(self, expected: ConsensusStatus<AuthorityCount>) -> Self {
		self.inspect_context(
			#[track_caller]
			|TestContext { umis, .. }| {
				assert!(!umis.is_empty(), "Asserted consensus on empty election set.");

				for umi in umis {
					let actual = MockElectoralSystem::consensus_status(*umi);
					assert_eq!(
					actual,
					expected,
					"Unexpected consensus status for election {:?} at {}.\nExpected: {:?}, Actual: {:?}",
					umi, core::panic::Location::caller(), expected, actual
				)
				}
			},
		)
	}
}

#[test]
fn consensus_state_transitions() {
	const VOTE: AuthorityVoteOf<MockElectoralSystem> = AuthorityVote::Vote(());

	election_test_ext(TestSetup { num_non_contributing_authorities: 2, ..Default::default() })
		.new_election()
		// Initial consensus state of the mock election system is `None`.
		.expect_consensus(ConsensusStatus::None)
		.assume_consensus()
		// Consensus is updated when we process a block's on_finalize hook.
		.expect_consensus(ConsensusStatus::None)
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Gained { most_recent: None, new: 0 })
		// After one vote we have consensus on the number of votes.
		.submit_votes(&[0], VOTE, Default::default())
		.expect_consensus(ConsensusStatus::Changed { previous: 0, new: 1 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 1 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 1 })
		// Another vote, consensus has changed.
		.submit_votes(&[1], VOTE, Default::default())
		.expect_consensus(ConsensusStatus::Changed { previous: 1, new: 2 })
		// Consensus is lost.
		.assume_no_consensus()
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 2 })
		.submit_votes(&[1], VOTE, Default::default()) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Lost { previous: 2 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::None)
		// Consensus is regained with the old value.
		.assume_consensus()
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::None)
		.submit_votes(&[1], VOTE, Default::default()) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Gained { most_recent: Some(2), new: 2 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 2 })
		// Consensus is lost.
		.assume_no_consensus()
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 2 })
		.submit_votes(&[1], VOTE, Default::default()) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Lost { previous: 2 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::None)
		// Consensus is regained with a new value.
		.assume_consensus()
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::None)
		.submit_votes(&[2], VOTE, Default::default()) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Gained { most_recent: Some(2), new: 3 })
		.then_process_next_block()
		.expect_consensus(ConsensusStatus::Unchanged { current: 3 })
		// Non-contributing authorities do not affect consensus.
		.submit_votes(&[3, 4], VOTE, Some(Error::<Test, _>::NotContributing))
		.expect_consensus(ConsensusStatus::Unchanged { current: 3 })
		.submit_simple(&[3, 4], |_| Call::<Test, _>::stop_ignoring_my_votes {})
		.submit_votes(&[3, 4], VOTE, None)
		.expect_consensus(ConsensusStatus::Changed { previous: 3, new: 5 });
}

#[test]
fn authority_removes_and_re_adds_itself_from_contributing_set() {
	const VOTE: AuthorityVoteOf<MockElectoralSystem> = AuthorityVote::Vote(());

	election_test_ext(Default::default())
		.new_election()
		.assume_consensus()
		.submit_votes(&[0, 1, 2], VOTE, None)
		.expect_consensus(ConsensusStatus::Gained { most_recent: None, new: 3 })
		.submit_simple(&[1], |_| Call::<Test, _>::ignore_my_votes {})
		.expect_consensus(ConsensusStatus::Changed { previous: 3, new: 2 })
		.submit_simple(&[1], |_| Call::<Test, _>::stop_ignoring_my_votes {})
		.expect_consensus(ConsensusStatus::Changed { previous: 2, new: 3 })
		// Validator 1 deletes its vote.
		.then_apply_extrinsics(
			#[track_caller]
			|TestContext { umis, .. }| {
				umis.iter()
					.map(|umi| {
						(
							OriginTrait::signed(1),
							Call::<Test, _>::delete_vote {
								election_identifier: ElectionIdentifier::new(*umi, ()),
							},
							Ok(()),
						)
					})
					.collect::<Vec<_>>()
			},
		)
		.expect_consensus(ConsensusStatus::Changed { previous: 3, new: 2 })
		.submit_simple(&[1], |_| Call::<Test, _>::ignore_my_votes {})
		.submit_votes(&[1], VOTE, Some(Error::<Test, _>::NotContributing))
		.expect_consensus(ConsensusStatus::Unchanged { current: 2 })
		.submit_simple(&[1], |_| Call::<Test, _>::stop_ignoring_my_votes {})
		.submit_votes(&[1], VOTE, None)
		.expect_consensus(ConsensusStatus::Changed { previous: 2, new: 3 });
}
