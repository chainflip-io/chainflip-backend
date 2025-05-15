// 1. We come to consensus when all data is the same.
// 2. We execute hooks when coming to consensus
// 3. On finalize we start as many elections as possible within the new range, but no more than the
//    maximum => skipping received blocks is fine, even ranges
// 4. First time the ES is run, we should only spawn the last election. Last procsesed will not
//    exist, so we have to ensure we don't generate
// all elections until the beginning of time.
// 5. Ranges are consistent, and if we're mid range we don't emit a new election, we wait for next
//    time we can election for a whole range.
// 6. State updates. When channel is opened, what happens? new election?
// Out of order consensus - when catching up this is possible. We need to ensure everything is still
// handled correctly.
// Testing with a chain with range > 1
// State partially processed, how do we test that the state still gets processed until all the state
// is processed.

use core::ops::RangeInclusive;

use super::{
	mocks::{Check, TestSetup},
	register_checks,
};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes, ElectoralSystemTypes},
	electoral_systems::{
		block_height_tracking::{ChainProgress, ChainProgressFor, ChainTypes},
		block_witnesser::{
			primitives::ElectionTracker,
			state_machine::{
				BWElectionProperties, BWProcessorTypes, ElectionPropertiesHook,
				ElectionTrackerEventHook, ExecuteHook, HookTypeFor, LogEventHook, RulesHook,
				SafeModeEnabledHook,
			},
			*,
		},
		mocks::ElectoralSystemState,
		state_machine::{
			core::{hook_test_utils::MockHook, Hook, TypesFor},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage,
};
use cf_chains::{mocks::MockEthereum, Chain};
use consensus::BWConsensus;
use primitives::SafeModeStatus;
use sp_std::collections::btree_set::BTreeSet;
use state_machine::{BWStatemachine, BWTypes, BlockWitnesserSettings, BlockWitnesserState};

fn range_n(start: u64, count: u64) -> RangeInclusive<u64> {
	assert!(count > 0);
	// TODO: Test with other witness ranges.
	start..=start + count - 1
}

type ChainBlockNumber = <MockEthereum as Chain>::ChainBlockNumber;
type ValidatorId = u16;
type BlockData = Vec<u8>;
type ElectionProperties = BTreeSet<u16>;
type ElectionCount = u16;

struct MockBlockProcessorDefinition;
type Types = TypesFor<MockBlockProcessorDefinition>;

impl Hook<HookTypeFor<Types, SafeModeEnabledHook>> for Types {
	fn run(&mut self, _input: ()) -> SafeModeStatus {
		SafeModeStatus::Disabled
	}
}

impl ChainTypes for Types {
	type ChainBlockNumber = u64;
	type ChainBlockHash = u64;
}

impl BWProcessorTypes for Types {
	type BlockData = Vec<u8>;
	type Event = ();
	type Rules = MockHook<HookTypeFor<Self, RulesHook>, "rules">;
	type Execute = MockHook<HookTypeFor<Self, ExecuteHook>, "execute">;
	type LogEventHook = MockHook<HookTypeFor<Self, LogEventHook>, "delete">;
}

/// Associating BW types to the struct
impl BWTypes for Types {
	type ElectionProperties = ElectionProperties;
	type ElectionPropertiesHook =
		MockHook<HookTypeFor<Self, ElectionPropertiesHook>, "generate_election_properties">;
	type SafeModeEnabledHook = Self;
	type ElectionTrackerEventHook = MockHook<HookTypeFor<Self, ElectionTrackerEventHook>>;
}

/// Associating the ES related types to the struct
impl ElectoralSystemTypes for Types {
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = u64;
	type ElectoralUnsynchronisedState = BlockWitnesserState<Self>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = BlockWitnesserSettings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = BWElectionProperties<Self>;
	type ElectionState = ();
	type VoteStorage =
		vote_storage::bitmap::Bitmap<(BlockData, Option<<Self as ChainTypes>::ChainBlockHash>)>;
	type Consensus = (BlockData, Option<<Self as ChainTypes>::ChainBlockHash>);
	type OnFinalizeContext = Vec<ChainProgressFor<Self>>;
	type OnFinalizeReturn = Vec<()>;
}

/// Associating the state machine and consensus mechanism to the struct
impl StatemachineElectoralSystemTypes for Types {
	// both context and return have to be vectors, these are the item types
	type OnFinalizeContextItem = ChainProgressFor<Self>;
	type OnFinalizeReturnItem = ();

	// the actual state machine and consensus mechanisms of this ES
	type Statemachine = BWStatemachine<Self>;
	type ConsensusMechanism = BWConsensus<Self>;
}

/// Generating the state machine-based electoral system
type SimpleBlockWitnesser = StatemachineElectoralSystem<Types>;

register_checks! {
	SimpleBlockWitnesser {
		generate_election_properties_called_n_times(pre, post, n: u8) {
			let pre_calls = pre.unsynchronised_state.generate_election_properties_hook.call_history.len();
			let post_calls = post.unsynchronised_state.generate_election_properties_hook.call_history.len();
			assert_eq!((post_calls - pre_calls) as u8, n, "generate_election_properties should have been called {} times in this `on_finalize`!", n);
		},
		number_of_open_elections_is(_pre, post, n: ElectionCount) {
			assert_eq!(post.unsynchronised_state.elections.ongoing.len(), n as usize, "Number of open elections should be {}", n);
		},
		rules_hook_called_n_times_for_age_zero(pre, post, n: usize) {
			let count = |state: &ElectoralSystemState<StatemachineElectoralSystem<TypesFor<MockBlockProcessorDefinition>>>| {
				state.unsynchronised_state.block_processor.rules.call_history.iter().filter(|(_, age, _event, _)| age.contains(&0)).count()
			};
			assert_eq!(count(post) - count(pre), n, "execute PreWitness event should have been called {} times in this `on_finalize`!", n);
		},
		// process_block_data_called_n_times(_pre, _post, n: u8) {
		// 	assert_eq!(PROCESS_BLOCK_DATA_HOOK_CALLED.with(|hook_called| hook_called.get()), n, "process_block_data should have been called {} times so far!", n);
		// },
		// process_block_data_called_last_with(_pre, _post, block_data: Vec<(ChainBlockNumber, BlockData)>) {
		// 	assert_eq!(PROCESS_BLOCK_DATA_CALLED_WITH.with(|old_block_data| old_block_data.borrow().clone()), block_data, "process_block_data should have been called with {:?}", block_data);
		// },
		// unprocessed_data_is(_pre, post, data: Vec<(ChainBlockNumber, BlockData)>) {
		// 	// assert_eq!(post.unsynchronised_state.unprocessed_data, data, "Unprocessed data should be {:?}", data);
		// },
		election_state_is(_pre, post) {
			println!("election state is: {:?}", post.unsynchronised_state.elections)
		}
	}
}

fn generate_votes(
	correct_voters: BTreeSet<ValidatorId>,
	incorrect_voters: BTreeSet<ValidatorId>,
	did_not_vote: BTreeSet<ValidatorId>,
	correct_data: BlockData,
) -> ConsensusVotes<SimpleBlockWitnesser> {
	println!("Generate votes called");

	let incorrect_data = vec![1u8, 2, 3];
	assert_ne!(incorrect_data, correct_data);
	let votes = ConsensusVotes {
		votes: correct_voters
			.clone()
			.into_iter()
			.map(|v| ConsensusVote {
				vote: Some(((), (correct_data.clone(), None))),
				validator_id: v,
			})
			.chain(incorrect_voters.clone().into_iter().map(|v| ConsensusVote {
				vote: Some(((), (incorrect_data.clone(), None))),
				validator_id: v,
			}))
			.chain(
				did_not_vote
					.clone()
					.into_iter()
					.map(|v| ConsensusVote { vote: None, validator_id: v }),
			)
			.collect(),
	};
	println!("correct voters: {:?}", correct_voters.len());
	println!("incorrect voters: {:?}", incorrect_voters.len());
	println!("did not vote: {:?}", did_not_vote.len());
	votes
}

// Util to create a successful set of votes, along with the consensus expectation.
fn create_votes_expectation(
	consensus: BlockData,
) -> (
	ConsensusVotes<SimpleBlockWitnesser>,
	Option<<SimpleBlockWitnesser as ElectoralSystemTypes>::Consensus>,
) {
	(
		generate_votes(
			(0..20).collect(),
			Default::default(),
			Default::default(),
			consensus.clone(),
		),
		Some((consensus, None)),
	)
}

const MAX_CONCURRENT_ELECTIONS: ElectionCount = 5;
const SAFETY_MARGIN: u32 = 3;

/*

// We start an election for a block and there is nothing there. The base case.
#[test]
fn no_block_data_success() {
	const NEXT_BLOCK_RECEIVED: ChainBlockNumber = 1;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN,
		})
		.build()
		.test_on_finalize(
			&vec![ChainProgress::FirstConsensus(range_n(NEXT_BLOCK_RECEIVED, 1))],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
			],
		)
		.expect_consensus(
			generate_votes((0..20).collect(), Default::default(), Default::default(), vec![]),
			Some(vec![]),
		)
		.test_on_finalize(
			&vec![ChainProgress::None],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(0),
				// No extra calls
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(0),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(1),
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
				// We should receive an empty block data, but still get the block number. This is
				// necessary so we can track the last chain block we've processed.
				// Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
				// 	NEXT_BLOCK_RECEIVED,
				// 	vec![],
				// )]),
			],
		);
}

#[test]
fn creates_multiple_elections_below_maximum_when_required() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	const NUMBER_OF_ELECTIONS: ElectionCount = MAX_CONCURRENT_ELECTIONS - 1;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, but still less than the maximum concurrent
			&vec![ChainProgress::FirstConsensus(range_n(
				INIT_LAST_BLOCK_RECEIVED,
				NUMBER_OF_ELECTIONS as u64,
			))],
			|pre_state| {
				assert_eq!(pre_state.unsynchronised_state.elections.ongoing.len(), 0);
			},
			vec![
				Check::<SimpleBlockWitnesser>::election_state_is(),
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(NUMBER_OF_ELECTIONS),
			],
		)
		.expect_consensus_multi(vec![
			(
				generate_votes((0..20).collect(), Default::default(), Default::default(), vec![]),
				Some(vec![]),
			),
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					Default::default(),
					vec![1, 3, 4],
				),
				Some(vec![1, 3, 4]),
			),
			// no progress on external chain but on finalize called again
		])
		.test_on_finalize(
			// same block again
			&vec![ChainProgress::None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					NUMBER_OF_ELECTIONS as usize
				);
			},
			vec![
				// Since two elections reached consensus and were closed,
				// we are left with `NUMBER_OF_ELECTIONS - 2` elections for which
				// we regenerate election properties in this `on_finalize`.
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8 - 2,
				),
				// we should have resolved two elections
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(NUMBER_OF_ELECTIONS - 2),
			],
		);
}

#[test]
fn creates_multiple_elections_limited_by_maximum() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	const NUMBER_OF_ELECTIONS_REQUIRED: ElectionCount = MAX_CONCURRENT_ELECTIONS * 2;
	let consensus_resolutions: Vec<(
		ConsensusVotes<SimpleBlockWitnesser>,
		Option<<Types as ElectoralSystemTypes>::Consensus>,
	)> = vec![
		create_votes_expectation(vec![]),
		create_votes_expectation(vec![1, 3, 4]),
		// no progress on external chain but on finalize called again
	];
	// let number_of_resolved_elections = consensus_resolutions.len();
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, but still elss than the maximum concurrent
			&vec![ChainProgress::Range(range_n(
				INIT_LAST_BLOCK_RECEIVED,
				NUMBER_OF_ELECTIONS_REQUIRED as u64,
			))],
			|pre_state| {
				assert_eq!(pre_state.unsynchronised_state.elections.ongoing.len(), 0);
			},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					MAX_CONCURRENT_ELECTIONS as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					MAX_CONCURRENT_ELECTIONS,
				),
			],
		)
		// Only resolve two of the elections. The last 3 are unresolved at this point. But
		// we now have space to start new elections.
		.expect_consensus_multi(consensus_resolutions)
		.test_on_finalize(
			&vec![ChainProgress::None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					MAX_CONCURRENT_ELECTIONS as usize
				);
			},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					MAX_CONCURRENT_ELECTIONS as u8,
				),
				// we should have resolved two elections
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					MAX_CONCURRENT_ELECTIONS,
				),
			],
		);
}

#[test]
fn reorg_clears_on_going_elections_and_continues() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 10;
	const NEXT_BLOCK_NUMBER: ChainBlockNumber =
		INIT_LAST_BLOCK_RECEIVED + MAX_CONCURRENT_ELECTIONS as u64;
	const REORG_LENGTH: ChainBlockNumber = 3;

	let all_votes = (INIT_LAST_BLOCK_RECEIVED + 1..=NEXT_BLOCK_NUMBER)
		.map(|_| create_votes_expectation(vec![5, 6, 7]))
		.collect::<Vec<_>>();

	// We have already emitted an election for `INIT_LAST_BLOCK_RECEIVED` (see TestSetup below), so
	// we add 1.
	let expected_unprocessed_data = (INIT_LAST_BLOCK_RECEIVED + 1..=NEXT_BLOCK_NUMBER)
		.map(|i| (i, vec![5, 6, 7]))
		.collect::<Vec<_>>();

	let mut block_after_reorg_block_unprocessed_data = expected_unprocessed_data.clone();
	block_after_reorg_block_unprocessed_data
		.push(((NEXT_BLOCK_NUMBER - REORG_LENGTH), vec![5, 6, 77]));

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState {
			elections: ElectionTracker {
				next_election: INIT_LAST_BLOCK_RECEIVED + 1,
				next_witnessed: INIT_LAST_BLOCK_RECEIVED + 1,
				..Default::default()
			},
			// last_block_election_emitted_for: INIT_LAST_BLOCK_RECEIVED,
			..BlockWitnesserState::default()
		})
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN,
		})
		.build()
		.test_on_finalize(
			&vec![ChainProgress::Range(range_n(
				INIT_LAST_BLOCK_RECEIVED + 1,
				MAX_CONCURRENT_ELECTIONS as u64,
			))],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					MAX_CONCURRENT_ELECTIONS as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					MAX_CONCURRENT_ELECTIONS,
				),
				// No reorg, so we try processing any unprocessed state (there would be none at
				// this point though, since no elections have resolved).
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
			],
		)
		.then(|| println!("We about to come to consensus on some blocks."))
		.expect_consensus_multi(all_votes)
		// Process votes as normal, progressing by one block, storing the state
		.test_on_finalize(
			&vec![ChainProgress::Range(range_n(NEXT_BLOCK_NUMBER + 1, 1))],
			|_| {},
			vec![
				// We've already processed the other elections, so we only have to create a new
				// election for the new block.
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
				// Check::<SimpleBlockWitnesser>::unprocessed_data_is(
				// 	expected_unprocessed_data.clone(),
				// ),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(5),
			],
		)
		.then(|| println!("We're about to come to consensus on a block that will trigger a reorg."))
		// Reorg occurs
		.test_on_finalize(
			&vec![ChainProgress::Range(range_n(
				// Range is inclusive, so for reorg length reorg, we need to -1 from reorg
				// length.
				(NEXT_BLOCK_NUMBER + 1) - (REORG_LENGTH - 1),
				REORG_LENGTH,
			))],
			|_| {},
			// We remove the actives ones and open one for the first block that we detected a
			// reorg for.
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					// REORG_LENGTH more than the last time we checked.
					REORG_LENGTH as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(REORG_LENGTH as u16),
				// We call it again, as even though there was a reorg, maybe some external state
				// changed that process_block_data uses, and now can process some of the
				// existing data.
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(3),
				// We keep the data, since it may need to be used by process_block_data to
				// deduplicate actions. We don't want to submit an action twice.
				// Check::<SimpleBlockWitnesser>::unprocessed_data_is(expected_unprocessed_data),
			],
		);
}

/*
#[test]
fn partially_processed_block_data_processed_next_on_finalize() {
	let first_block_consensus: BlockData = vec![5, 6, 7];

	let first_block_data_after_processing: Vec<_> =
		first_block_consensus.clone().into_iter().take(2).collect();

	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState {
			last_block_received: INIT_LAST_BLOCK_RECEIVED,
			..BlockWitnesserState::default()
		})
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
		})
		.build()
		.test_on_finalize(
			&range_n(INIT_LAST_BLOCK_RECEIVED + 1, 1),
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
				// We haven't come to consensus on any elections, so there's no unprocessed data.
				// Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![]),
			],
		)
		.expect_consensus_multi(vec![create_votes_expectation(first_block_consensus.clone())])
		.then(|| {
			// We process one of the items, so we return only 2 of 3.
			MockBlockProcessor::set_block_data_to_return(vec![(
				INIT_LAST_BLOCK_RECEIVED + 1,
				first_block_data_after_processing.clone(),
			)]);
		})
		.test_on_finalize(
			&range_n(INIT_LAST_BLOCK_RECEIVED + 2),
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
				// One opened, one closed.
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// We call it again.
				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
				// We have the election data for the election we emitted before now. We try to
				// process it.
				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
					INIT_LAST_BLOCK_RECEIVED + 1,
					first_block_consensus,
				)]),
			],
		)
		// No progress on external chain, so state should be the same as above, except that we
		// processed one of the items last time.
		.test_on_finalize(
			&range_n(INIT_LAST_BLOCK_RECEIVED + 2),
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// We call it again.
				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(3),
				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
					INIT_LAST_BLOCK_RECEIVED + 1,
					first_block_data_after_processing,
				)]),
			],
		);
}
 */

#[test]
fn elections_resolved_out_of_order_has_no_impact() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	const NUMBER_OF_ELECTIONS: ElectionCount = 2;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState {
			elections: ElectionTracker {
				next_election: INIT_LAST_BLOCK_RECEIVED + 1,
				next_witnessed: INIT_LAST_BLOCK_RECEIVED + 1,
				..Default::default()
			},
			// last_block_election_emitted_for: INIT_LAST_BLOCK_RECEIVED,
			..BlockWitnesserState::default()
		})
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, but still less than the maximum concurrent
			&vec![ChainProgress::Range(range_n(
				INIT_LAST_BLOCK_RECEIVED + 1,
				NUMBER_OF_ELECTIONS as u64,
			))],
			|pre_state| {
				assert_eq!(pre_state.unsynchronised_state.elections.ongoing.len(), 0);
			},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(NUMBER_OF_ELECTIONS),
			],
		)
		.expect_consensus_multi(vec![
			(
				// no consensus
				generate_votes((0..20).collect(), (0..20).collect(), Default::default(), vec![]),
				None,
			),
			(
				// consensus
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![1, 3, 4],
				),
				Some(vec![1, 3, 4]),
			),
		])
		// no progress on external chain but on finalize called again
		// TODO: Check the new elections have kicked off correct
		.test_on_finalize(
			&vec![ChainProgress::Range(range_n(
				INIT_LAST_BLOCK_RECEIVED + 1 + (NUMBER_OF_ELECTIONS as u64),
				1,
			))],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					NUMBER_OF_ELECTIONS as usize
				);
			},
			vec![
				// one extra election created
				// TODO: this should check that we actually called properties for the newly created
				// election. as things are we cannot see whether we closed an election and
				// opened a new one or simply kept the previously ongoing ones
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8,
				),
				// we should have resolved one election, and started one election
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
				// Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![(
				// 	SECOND_ELECTION_BLOCK_CREATED,
				// 	vec![1, 3, 4],
				// )]),
			],
		)
		// gain consensus on the first emitted election now
		.expect_consensus_multi(vec![(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![9, 1, 2],
			),
			Some(vec![9, 1, 2]),
		)])
		.test_on_finalize(
			&vec![ChainProgress::Range(range_n(
				INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64) + 2,
				1,
			))],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					2,
					"number of open elections should be 2"
				);
			},
			vec![
				// one extra election created
				// TODO, same as above TODO
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8,
				),
				// we should have resolved one elections, and started one election
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
				// Now the first election we emitted is resolved, and its block data should be
				// stored, and we should still have the second election block data.
				// Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![
				// 	(SECOND_ELECTION_BLOCK_CREATED, vec![1, 3, 4]),
				// 	(FIRST_ELECTION_BLOCK_CREATED, vec![9, 1, 2]),
				// ]),
			],
		)
		// Gain consensus on the final elections
		.expect_consensus_multi(vec![
			(
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![81, 1, 93],
				),
				Some(vec![81, 1, 93]),
			),
			(
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![69, 69, 69],
				),
				Some(vec![69, 69, 69]),
			),
		])
		// external chain doesn't move forward
		.test_on_finalize(
			&vec![ChainProgress::None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					2,
					"number of open elections should be 2"
				);
			},
			vec![
				// no new election created
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(0),
				// all elections have resolved now
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(0),
				// Now the last two elections are resolved in order
				// Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![
				// 	(SECOND_ELECTION_BLOCK_CREATED, vec![1, 3, 4]),
				// 	(FIRST_ELECTION_BLOCK_CREATED, vec![9, 1, 2]),
				// 	(SECOND_ELECTION_BLOCK_CREATED + 1, vec![81, 1, 93]),
				// 	(SECOND_ELECTION_BLOCK_CREATED + 2, vec![69, 69, 69]),
				// ]),
			],
		);
}
 */
