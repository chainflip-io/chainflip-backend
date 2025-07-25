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

#![cfg(test)]
use crate::{mock::*, *};
use cf_primitives::AuthorityCount;
use electoral_system::ConsensusStatus;
use electoral_system_runner::RunnerStorageAccessTrait;
use electoral_systems::mock::{BehaviourUpdate, MockElectoralSystemRunner};
use frame_support::traits::OriginTrait;
use mock::Test;
use std::collections::BTreeMap;
use vote_storage::AuthorityVote;

#[test]
fn votes_not_provided_until_shared_data_is_provided() {
	let setup = TestSetup::default();
	let authorities = setup.all_authorities();
	let initial_test_state = election_test_ext(setup)
		.then_apply_extrinsics(|_| {
			[(
				OriginTrait::root(),
				Call::<Test, _>::set_shared_data_reference_lifetime {
					blocks: 10,
					ignore_corrupt_storage: CorruptStorageAdherance::Heed,
				},
				Ok(()),
			)]
		})
		.new_election()
		.assert_calls_noop(
			&authorities[..],
			|_| Call::<_, _>::provide_shared_data { shared_data: Box::new(()) },
			Error::<Test, _>::UnreferencedSharedData,
		)
		.assume_consensus()
		.expect_consensus(ConsensusStatus::None)
		// Partial Vote does not contain shared data, only the reference.
		.submit_votes(&authorities[..], AuthorityVote::PartialVote(SharedDataHash::of(&())), Ok(()))
		// No votes are provided to the consensus system because shared component has not been
		// provided.
		.expect_consensus(ConsensusStatus::Gained { most_recent: None, new: 0 })
		.then_execute_with_keep_context(|_| {
			let electoral_data = Pallet::<Test, Instance1>::electoral_data(&authorities[0])
				.expect("Expected electoral data.");
			assert_eq!(electoral_data.current_elections.len(), 1, "Expected one election.");
			assert_eq!(
				electoral_data.unprovided_shared_data_hashes.len(),
				1,
				"Expected one shared data hash."
			);
		})
		// Delete the election when we finalize: should cause all refs to be deleted too.
		.update_settings(&[BehaviourUpdate::DeleteOnFinalizeConsensus(true)])
		.snapshot();

	// Case 1: Provide Shared Data through provide_shared_data extrinsic:
	let case_1 = TestRunner::from_snapshot(initial_test_state.clone())
		.assert_calls_ok(&authorities[..1], |_| Call::<Test, Instance1>::provide_shared_data {
			shared_data: Box::new(()),
		});

	// Case 2: Provide Shared Data through vote extrinsic:
	let case_2 = TestRunner::from_snapshot(initial_test_state.clone()).submit_votes(
		&authorities[..1],
		AuthorityVote::Vote(()),
		Ok(()),
	);

	for (label, test_case) in [(1, case_1), (2, case_2)] {
		test_case
			// Shared data provided, all votes should now be counted.
			.expect_consensus(ConsensusStatus::Changed {
				previous: 0,
				new: authorities.len() as AuthorityCount,
			})
			.then_execute_with_keep_context(|_| {
				assert!(
					SharedDataReferenceCount::<Test, _>::iter().next().is_none(),
					"Case {label}: Expected shared data refs to be removed but found: {:?}, components are: {:?}",
					SharedDataReferenceCount::<Test, _>::iter().collect::<Vec<_>>(),
					IndividualComponents::<Test, _>::iter().collect::<Vec<_>>(),
				);
			});
	}
}

#[test]
fn ensure_can_vote() {
	new_test_ext().then_execute_at_next_block(|()| {
		let setup = TestSetup { num_non_contributing_authorities: 1, ..Default::default() };

		let initial_state = election_test_ext(setup.clone())
			.new_election()
			.submit_votes(
				&setup.non_contributing_authorities()[..],
				AuthorityVote::Vote(()),
				Err(Error::NotContributing),
			)
			.snapshot();

		// Contributing authorities can vote.
		TestRunner::from_snapshot(initial_state.clone()).submit_votes(
			&setup.contributing_authorities()[..],
			AuthorityVote::Vote(()),
			Ok(()),
		);

		// If governance pauses elections, no votes can be submitted.
		TestRunner::from_snapshot(initial_state.clone())
			.then_apply_extrinsics(|_| {
				[(OriginTrait::root(), Call::<Test, _>::pause_elections {}, Ok(()))]
			})
			.submit_votes(
				&setup.all_authorities()[..],
				AuthorityVote::Vote(()),
				Err(Error::Paused),
			);
	});
}

pub trait ElectoralSystemRunnerTestExt: Sized {
	fn update_settings(self, updates: &[BehaviourUpdate]) -> Self;
	fn expect_consensus_after_next_block(self, expected: ConsensusStatus<AuthorityCount>) -> Self;
	fn assume_consensus(self) -> Self;
	fn assume_no_consensus(self) -> Self;
	fn expect_consensus(self, expected: ConsensusStatus<AuthorityCount>) -> Self;
	fn new_election(self) -> Self;
	fn submit_votes<I: 'static>(
		self,
		validator_ids: &[u64],
		vote: AuthorityVoteOf<MockElectoralSystemRunner>,
		expected_outcome: Result<(), Error<Test, I>>,
	) -> Self
	where
		Test: Config<I, ElectoralSystemRunner = MockElectoralSystemRunner>,
		<Test as frame_system::Config>::RuntimeCall: From<Call<Test, I>>;
}

impl ElectoralSystemRunnerTestExt for TestRunner<TestContext> {
	/// Starts a new election, adding its unique monotonic identifier to the test context.
	#[track_caller]
	fn new_election(self) -> Self {
		self.then_execute_with(
			#[track_caller]
			|mut ctx| {
				let identifier =
					RunnerStorageAccess::<Test, Instance1>::new_election((), (), ()).unwrap();
				let unique_monotonic_identifier = identifier.unique_monotonic();

				assert_eq!(Status::<Test, Instance1>::get(), Some(ElectionPalletStatus::Running));

				ctx.umis.push(*unique_monotonic_identifier);

				ctx
			},
		)
	}

	fn update_settings(self, updates: &[BehaviourUpdate]) -> Self {
		MockElectoralSystemRunner::update(updates);
		self
	}

	#[track_caller]
	fn assume_consensus(self) -> Self {
		self.update_settings(&[BehaviourUpdate::AssumeConsensus(true)])
	}

	#[track_caller]
	fn assume_no_consensus(self) -> Self {
		self.update_settings(&[BehaviourUpdate::AssumeConsensus(false)])
	}

	#[track_caller]
	fn submit_votes<I: 'static>(
		self,
		validator_ids: &[u64],
		vote: AuthorityVoteOf<MockElectoralSystemRunner>,
		expected_outcome: Result<(), Error<Test, I>>,
	) -> Self
	where
		Test: Config<I, ElectoralSystemRunner = MockElectoralSystemRunner>,
		<Test as frame_system::Config>::RuntimeCall: From<Call<Test, I>>,
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
									authority_votes:
										Box::new(
											BoundedBTreeMap::try_from(
												sp_std::iter::once(
													(
														ElectionIdentifierOf::<
															MockElectoralSystemRunner,
														>::new(*umi, ()),
														vote.clone(),
													),
												)
												.collect::<BTreeMap<_, _>>(),
											)
											.unwrap(),
										),
								},
								expected_outcome.clone().map_err(Into::into),
							)
						})
					})
					.collect::<Vec<_>>()
			},
		)
	}

	#[track_caller]
	fn expect_consensus(self, expected: ConsensusStatus<AuthorityCount>) -> Self {
		self.inspect_context(
			#[track_caller]
			|TestContext { umis, .. }| {
				assert!(!umis.is_empty(), "Asserted consensus on empty election set.");

				for umi in umis {
					let actual = MockElectoralSystemRunner::consensus_status(*umi);
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

	/// Processes a single block, then checks the consensus status.
	#[track_caller]
	fn expect_consensus_after_next_block(self, expected: ConsensusStatus<AuthorityCount>) -> Self {
		self.then_process_next_block().expect_consensus(expected)
	}
}

#[test]
fn consensus_state_transitions() {
	const VOTE: AuthorityVoteOf<MockElectoralSystemRunner> = AuthorityVote::Vote(());

	election_test_ext(TestSetup { num_non_contributing_authorities: 2, ..Default::default() })
		.new_election()
		// Initial consensus state of the mock election system is `None`.
		.expect_consensus(ConsensusStatus::None)
		.assume_consensus()
		// Consensus is updated when we process a block's on_finalize hook.
		.expect_consensus(ConsensusStatus::None)
		.expect_consensus_after_next_block(ConsensusStatus::Gained { most_recent: None, new: 0 })
		// After one vote we have consensus on the number of votes.
		.submit_votes(&[0], VOTE, Ok(()))
		.expect_consensus(ConsensusStatus::Changed { previous: 0, new: 1 })
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 1 })
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 1 })
		// Another vote, consensus has changed.
		.submit_votes(&[1], VOTE, Ok(()))
		.expect_consensus(ConsensusStatus::Changed { previous: 1, new: 2 })
		// Consensus is lost.
		.assume_no_consensus()
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 2 })
		.submit_votes(&[1], VOTE, Ok(())) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Lost { previous: 2 })
		.expect_consensus_after_next_block(ConsensusStatus::None)
		// Consensus is regained with the old value.
		.assume_consensus()
		.expect_consensus_after_next_block(ConsensusStatus::None)
		.submit_votes(&[1], VOTE, Ok(())) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Gained { most_recent: Some(2), new: 2 })
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 2 })
		// Consensus is lost.
		.assume_no_consensus()
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 2 })
		.submit_votes(&[1], VOTE, Ok(())) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Lost { previous: 2 })
		.expect_consensus_after_next_block(ConsensusStatus::None)
		// Consensus is regained with a new value.
		.assume_consensus()
		.expect_consensus_after_next_block(ConsensusStatus::None)
		.submit_votes(&[2], VOTE, Ok(())) // Consensus is only updated if there is a vote.
		.expect_consensus(ConsensusStatus::Gained { most_recent: Some(2), new: 3 })
		.expect_consensus_after_next_block(ConsensusStatus::Unchanged { current: 3 })
		// Non-contributing authorities do not affect consensus.
		.submit_votes(&[3, 4], VOTE, Err(Error::<Test, _>::NotContributing))
		.expect_consensus(ConsensusStatus::Unchanged { current: 3 })
		.assert_calls_ok(&[3, 4], |_| Call::<Test, _>::stop_ignoring_my_votes {})
		.submit_votes(&[3, 4], VOTE, Ok(()))
		.expect_consensus(ConsensusStatus::Changed { previous: 3, new: 5 });
}

#[test]
fn authority_removes_and_re_adds_itself_from_contributing_set() {
	const VOTE: AuthorityVoteOf<MockElectoralSystemRunner> = AuthorityVote::Vote(());

	election_test_ext(Default::default())
		.new_election()
		.assume_consensus()
		.submit_votes(&[0, 1, 2], VOTE, Ok(()))
		.expect_consensus(ConsensusStatus::Gained { most_recent: None, new: 3 })
		.assert_calls_ok(&[1], |_| Call::<Test, _>::ignore_my_votes {})
		.expect_consensus(ConsensusStatus::Changed { previous: 3, new: 2 })
		.assert_calls_ok(&[1], |_| Call::<Test, _>::stop_ignoring_my_votes {})
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
		.assert_calls_ok(&[1], |_| Call::<Test, _>::ignore_my_votes {})
		.submit_votes(&[1], VOTE, Err(Error::<Test, _>::NotContributing))
		.expect_consensus(ConsensusStatus::Unchanged { current: 2 })
		.assert_calls_ok(&[1], |_| Call::<Test, _>::stop_ignoring_my_votes {})
		.submit_votes(&[1], VOTE, Ok(()))
		.expect_consensus(ConsensusStatus::Changed { previous: 2, new: 3 });
}
