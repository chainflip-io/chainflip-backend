// 1. We come to consensus when all data is the same.
// 2. We execute hooks when coming to consensus
// 3. On finalize we start as many elections as possible within the new range, but no more than the
//    maximum => skipping received blocks is fine, even ranges
// 4. First time the ES is run, we should only spawn the last election. Last processed will not
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

use core::ops::{Range, RangeInclusive};

use super::{mocks::Check, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes, ElectoralSystemTypes},
	electoral_systems::{
		block_height_witnesser::{primitives::Header, ChainProgress, ChainTypes},
		block_witnesser::{
			primitives::ElectionTracker,
			state_machine::{
				BWElectionProperties, BWElectionType, BWProcessorTypes, BlockWitnesserSettings,
				BlockWitnesserState, DebugEventHook, ElectionPropertiesHook,
				ElectionTrackerDebugEventHook, ExecuteHook, HookTypeFor, RulesHook,
				SafeModeEnabledHook,
			},
			*,
		},
		mocks::{ElectoralSystemState, TestSetup},
		state_machine::{
			consensus::{ConsensusMechanism, SuccessThreshold},
			core::{
				hook_test_utils::{ConstantHook, MockHook},
				Hook, HookType, TypesFor,
			},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage,
};
use consensus::BWConsensus;
use primitives::SafeModeStatus;
use sp_std::collections::btree_set::BTreeSet;
use state_machine::{BWStatemachine, BWTypes};

type ChainBlockNumber = <Types as ChainTypes>::ChainBlockNumber;
type ValidatorId = u16;
type BlockData = Vec<u8>;
type BlockHash = u64;
type ElectionProperties = BTreeSet<u16>;
type ElectionCount = u16;

struct MockBlockProcessorDefinition;
type Types = TypesFor<MockBlockProcessorDefinition>;

impl Hook<HookTypeFor<Types, RulesHook>> for Types {
	fn run(
		&mut self,
		(ages, data, _safety_margin): <HookTypeFor<Types, RulesHook> as HookType>::Input,
	) -> <HookTypeFor<Types, RulesHook> as HookType>::Output {
		if ages.contains(&0) {
			data
		} else {
			Vec::new()
		}
	}
}

impl ChainTypes for Types {
	type ChainBlockNumber = u64;
	type ChainBlockHash = u64;

	const SAFETY_BUFFER: usize = SAFETY_MARGIN * 2;
}

impl BWProcessorTypes for Types {
	type Chain = Types;
	type BlockData = Vec<u8>;
	type Event = u8;
	type Rules = MockHook<HookTypeFor<Self, RulesHook>, "rules", Self>;
	type Execute = MockHook<HookTypeFor<Self, ExecuteHook>, "execute">;
	type DebugEventHook = MockHook<HookTypeFor<Self, DebugEventHook>, "debug">;
}

/// Associating BW types to the struct
impl BWTypes for Types {
	type ElectionProperties = ElectionProperties;
	type ElectionPropertiesHook =
		MockHook<HookTypeFor<Self, ElectionPropertiesHook>, "generate_election_properties">;
	type SafeModeEnabledHook = MockHook<HookTypeFor<Self, SafeModeEnabledHook>, "safe_mode">;
	type ElectionTrackerDebugEventHook = MockHook<HookTypeFor<Self, ElectionTrackerDebugEventHook>>;
}

/// Associating the state machine and consensus mechanism to the struct
impl StatemachineElectoralSystemTypes for Types {
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = u64;
	type VoteStorage =
		vote_storage::bitmap::Bitmap<(BlockData, Option<<Self as ChainTypes>::ChainBlockHash>)>;
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
				state.unsynchronised_state.block_processor.rules.call_history.iter().filter(|(age, _data, _event)| age.contains(&0)).count()
			};
			assert_eq!(count(post) - count(pre), n, "execute PreWitness event should have been called {} times in this `on_finalize`!", n);
		},
		emitted_prewitness_events(pre, post, events: Vec<(ChainBlockNumber, Vec<u8>)>) {
			let get_events = |state: &ElectoralSystemState<StatemachineElectoralSystem<TypesFor<MockBlockProcessorDefinition>>>| {
				state.unsynchronised_state.block_processor.execute.call_history.iter().flatten().cloned().collect::<Vec<_>>()
			};
			let actual_events = get_events(post).into_iter().skip(get_events(pre).len()).collect::<Vec<_>>();
			assert_eq!(actual_events, events.into_iter().flat_map(|(block_num, values)| { values.into_iter().map(move |value| (block_num, value)) }).collect::<Vec<_>>(), "emitted prewitness events not correct!");
		},
		open_elections_type_is(_pre, post, param: Vec<(ChainBlockNumber, BWElectionType<Types>)>) {
			let get_election = |state: &ElectoralSystemState<StatemachineElectoralSystem<TypesFor<MockBlockProcessorDefinition>>>, n: ChainBlockNumber| {
				let election = state.unsynchronised_state.elections.ongoing.get(&n);
				assert!(election.is_some(), "No election present for block {:?}", n);
				election.unwrap().clone()
			};
			for election in param {
				let (n, election_type) = election;
				assert_eq!(get_election(post, n), election_type, "election should be of type {:?}", election_type);
			}
		}
	}
}

fn generate_votes(
	correct_voters: BTreeSet<ValidatorId>,
	incorrect_voters: BTreeSet<ValidatorId>,
	did_not_vote: BTreeSet<ValidatorId>,
	correct_data: BlockData,
	block_hash: Option<BlockHash>,
) -> ConsensusVotes<SimpleBlockWitnesser> {
	println!("Generate votes called");

	let incorrect_data = vec![1u8, 2, 3];
	assert_ne!(incorrect_data, correct_data);
	let votes = ConsensusVotes {
		votes: correct_voters
			.clone()
			.into_iter()
			.map(|v| ConsensusVote {
				vote: Some(((), (correct_data.clone(), block_hash))),
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
#[allow(dead_code)]
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
			None,
		),
		Some((consensus, None)),
	)
}

const MAX_CONCURRENT_ELECTIONS: ElectionCount = 5;
const SAFETY_MARGIN: usize = 3;
const MOCK_BW_ELECTION_PROPERTIES: BWElectionProperties<Types> = BWElectionProperties {
	election_type: state_machine::EngineElectionType::<Types>::BlockHeight { submit_hash: false },
	block_height: 2,
	properties: BTreeSet::new(),
};

#[test]
fn block_witnesser_consensus() {
	let mut bw_consensus: BWConsensus<Types> = Default::default();
	bw_consensus.insert_vote((vec![1, 3, 5], Some(2)));
	bw_consensus.insert_vote((vec![1, 3, 5], Some(2)));
	bw_consensus.insert_vote((vec![1, 3], Some(2)));
	let consensus = bw_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, MOCK_BW_ELECTION_PROPERTIES));
	assert_eq!(consensus, None);

	bw_consensus.insert_vote((vec![1, 3, 5], Some(3)));
	let consensus = bw_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, MOCK_BW_ELECTION_PROPERTIES));
	assert_eq!(consensus, None);

	bw_consensus.insert_vote((vec![1, 3, 5], Some(2)));
	let consensus = bw_consensus
		.check_consensus(&(SuccessThreshold { success_threshold: 3 }, MOCK_BW_ELECTION_PROPERTIES));
	assert_eq!(consensus, Some((vec![1, 3, 5], Some(2))));
}

// We start an election for a block, receive it and emit PreWitness events for it. The base case.
// (no optimistic elections)
#[test]
fn election_starts_and_resolves_when_consensus_is_reached() {
	const NEXT_HEADER_RECEIVED: Header<Types> = Header { block_height: 1, hash: 1, parent_hash: 0 };
	const TX_RECEIVED: u8 = 42;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			max_optimistic_elections: 0,
			safety_margin: SAFETY_MARGIN as u32,
		})
		.build()
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: [NEXT_HEADER_RECEIVED].into(), removed: None })],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![]),
			],
		)
		.expect_consensus(
			generate_votes(
				(0..20).collect(),
				Default::default(),
				Default::default(),
				vec![TX_RECEIVED],
				None,
			),
			Some((vec![TX_RECEIVED], None)),
		)
		.test_on_finalize(
			&vec![None],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(0),
				// No extra calls
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(0),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(1),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![(
					1,
					vec![TX_RECEIVED],
				)]),
			],
		);
}

#[test]
fn creates_multiple_elections_below_maximum_when_required() {
	const HEADERS_RECEIVED: [Header<Types>; 3] = [
		Header { block_height: 1, hash: 1, parent_hash: 0 },
		Header { block_height: 2, hash: 2, parent_hash: 1 },
		Header { block_height: 3, hash: 3, parent_hash: 2 },
	];
	assert!(HEADERS_RECEIVED.len() < MAX_CONCURRENT_ELECTIONS as usize);

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN as u32,
			max_optimistic_elections: 0,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, but still less than the maximum concurrent
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
			|pre_state| {
				assert_eq!(pre_state.unsynchronised_state.elections.ongoing.len(), 0);
			},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					HEADERS_RECEIVED.len() as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					HEADERS_RECEIVED.len() as u16
				),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![
					(1, BWElectionType::ByHash(1)),
					(2, BWElectionType::ByHash(2)),
					(3, BWElectionType::ByHash(3)),
				]),
			],
		)
		.expect_consensus_multi(vec![
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					Default::default(),
					vec![],
					None,
				),
				Some((vec![], None)),
			),
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					Default::default(),
					vec![1, 3, 4],
					None,
				),
				Some((vec![1, 3, 4], None)),
			),
			// no progress on external chain but on finalize called again
		])
		.test_on_finalize(
			// same block again
			&vec![None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					HEADERS_RECEIVED.len()
				);
			},
			vec![
				// Since two elections reached consensus and were closed,
				// we are left with `NUMBER_OF_ELECTIONS - 2` elections for which
				// we regenerate election properties in this `on_finalize`.
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					HEADERS_RECEIVED.len() as u8 - 2,
				),
				// we should have resolved two elections
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					HEADERS_RECEIVED.len() as u16 - 2,
				),
			],
		);
}

#[test]
fn creates_multiple_elections_limited_by_maximum() {
	const NUMBER_OF_ELECTIONS_REQUIRED: ElectionCount = MAX_CONCURRENT_ELECTIONS * 2;
	const HEADERS_RECEIVED: [Header<Types>; NUMBER_OF_ELECTIONS_REQUIRED as usize] = [
		Header { block_height: 1, hash: 1, parent_hash: 0 },
		Header { block_height: 2, hash: 2, parent_hash: 1 },
		Header { block_height: 3, hash: 3, parent_hash: 2 },
		Header { block_height: 4, hash: 4, parent_hash: 3 },
		Header { block_height: 5, hash: 5, parent_hash: 4 },
		Header { block_height: 6, hash: 6, parent_hash: 5 },
		Header { block_height: 7, hash: 7, parent_hash: 6 },
		Header { block_height: 8, hash: 8, parent_hash: 7 },
		Header { block_height: 9, hash: 9, parent_hash: 8 },
		Header { block_height: 10, hash: 10, parent_hash: 9 },
	];

	let consensus_resolutions =
		vec![create_votes_expectation(vec![]), create_votes_expectation(vec![1, 3, 4])];

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			max_optimistic_elections: 0,
			safety_margin: SAFETY_MARGIN as u32,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, more than the maximum concurrent
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
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
				// SAFETY_BUFFER is SAFETY_MARGIN*2 (6) hence blocks "older" than latest_height -
				// SAFETY_BUFFER are queried through SafeBlockHeight elections
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![
					(1, BWElectionType::SafeBlockHeight),
					(2, BWElectionType::SafeBlockHeight),
					(3, BWElectionType::SafeBlockHeight),
					(4, BWElectionType::SafeBlockHeight),
					(5, BWElectionType::ByHash(5)),
				]),
			],
		)
		// Only resolve two of the elections. The last 3 are unresolved at this point. But
		// we now have space to start new elections.
		.expect_consensus_multi(consensus_resolutions)
		.test_on_finalize(
			&vec![None],
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
	const HEADERS_RECEIVED: [Header<Types>; MAX_CONCURRENT_ELECTIONS as usize] = [
		Header { block_height: 11, hash: 11, parent_hash: 10 },
		Header { block_height: 12, hash: 12, parent_hash: 11 },
		Header { block_height: 13, hash: 13, parent_hash: 12 },
		Header { block_height: 14, hash: 14, parent_hash: 13 },
		Header { block_height: 15, hash: 15, parent_hash: 14 },
	];
	const NEXT_HEADER_RECEIVED: [Header<Types>; 1] =
		[Header { block_height: 16, hash: 16, parent_hash: 15 }];
	const REORG_LENGTH: usize = 3;
	const REORG_HEADERS_REMOVED: RangeInclusive<ChainBlockNumber> = 14..=13 + REORG_LENGTH as u64;
	const REORG_NEW_HEADERS_RECEIVED: [Header<Types>; REORG_LENGTH] = [
		Header { block_height: 14, hash: 140, parent_hash: 13 },
		Header { block_height: 15, hash: 150, parent_hash: 140 },
		Header { block_height: 16, hash: 160, parent_hash: 150 },
	];
	const POST_REORG_HEADER_RECEIVED: [Header<Types>; 1] =
		[Header { block_height: 17, hash: 17, parent_hash: 160 }];

	const PRE_REORG_RECEIVED_TXS: [Range<u8>; MAX_CONCURRENT_ELECTIONS as usize] =
		[5..8, 8..11, 11..14, 14..17, 17..20];

	// there's a special tx that only appears post-reorg in a reorged block
	const SPECIAL_POST_REORG_TX: u8 = 200;
	assert!(PRE_REORG_RECEIVED_TXS
		.iter()
		.all(|range| !range.contains(&SPECIAL_POST_REORG_TX)));

	let all_votes = PRE_REORG_RECEIVED_TXS
		.iter()
		.map(|range| create_votes_expectation(range.clone().collect()))
		.collect::<Vec<_>>();

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN as u32,
			max_optimistic_elections: 0,
		})
		.build()
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
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
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
			],
		)
		.then(|| println!("We about to come to consensus on some blocks."))
		.expect_consensus_multi(all_votes)
		// Process votes as normal, progressing by one block, storing the state
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: NEXT_HEADER_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				// We've already processed the other elections, so we only have to create a new
				// election for the new block.
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(5),

				// Ensure that we emit prewitness events for all txs from all the votes that reached consensus
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![
					(11, vec![5,6,7]),
					(12, vec![8,9,10]),
					(13, vec![11,12,13]),
					(14, vec![14,15,16]),
					(15, vec![17,18,19]),
					]
				),
			],
		)
		.then(|| println!("We're about to come to consensus on a block that will trigger a reorg."))
		// Reorg occurs
		.test_on_finalize(
			&vec![Some(ChainProgress {
				headers: REORG_NEW_HEADERS_RECEIVED.into(),
				removed: Some(REORG_HEADERS_REMOVED),
			})],
			|_| {},
			// We reopen an election for reorged blocks
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					// REORG_LENGTH more than the last time we checked.
					REORG_LENGTH as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(REORG_LENGTH as u16),
			],
		)
		.then(|| {
			println!("We're about to come to consensus and the deposits end up in another block, there's also a new one")
		})
		.expect_consensus_multi(vec![
			create_votes_expectation(vec![14, 15, SPECIAL_POST_REORG_TX]),
			create_votes_expectation(vec![18]),
			create_votes_expectation(vec![19, 16]),
		])
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: POST_REORG_HEADER_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				//we proceed as normal and open election for the new header
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// Emit only prewitness events for the one deposit that is new post-reorg
				// All other deposits have been prewitnessed before.
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![
					(14, vec![SPECIAL_POST_REORG_TX])
				])
			],
		);
}

#[test]
fn elections_resolved_out_of_order_has_no_impact() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	const NUMBER_OF_ELECTIONS: ElectionCount = 2;
	const HEADERS_RECEIVED: [Header<Types>; NUMBER_OF_ELECTIONS as usize] = [
		Header { block_height: 1, hash: 1, parent_hash: 0 },
		Header { block_height: 2, hash: 2, parent_hash: 1 },
	];
	const NEXT_HEADER_RECEIVED: [Header<Types>; 1] =
		[Header { block_height: 3, hash: 3, parent_hash: 2 }];

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState {
			elections: ElectionTracker {
				seen_heights_below: INIT_LAST_BLOCK_RECEIVED + 1,
				..Default::default()
			},
			..BlockWitnesserState::default()
		})
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN as u32,
			max_optimistic_elections: 0,
		})
		.build()
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
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
				generate_votes(
					(0..20).collect(),
					(20..40).collect(),
					Default::default(),
					vec![9, 1, 2],
					None,
				),
				None,
			),
			(
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![1, 3, 4],
					None,
				),
				Some((vec![1, 3, 4], None)),
			),
		])
		// no progress on external chain but on finalize called again
		.test_on_finalize(
			&vec![None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					NUMBER_OF_ELECTIONS as usize
				);
			},
			vec![
				// one extra election created for the block that we didn't reach consensus over
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS.saturating_sub(1) as u8,
				),
				// we should have resolved one election, and so there's one less ongoing
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					NUMBER_OF_ELECTIONS.saturating_sub(1),
				),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![(2, vec![1, 3, 4])]),
			],
		)
		// gain consensus on the first emitted election now
		.expect_consensus_multi(vec![(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![9, 1, 2],
				None,
			),
			Some((vec![9, 1, 2], None)),
		)])
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: NEXT_HEADER_RECEIVED.into(), removed: None })],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					1,
					"number of open elections should be 1"
				);
			},
			vec![
				// one extra election created
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				// we should have resolved one elections, and started one election
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// Now the first election we emitted is resolved, and its block data should be
				// stored, and we should still have the second election block data.
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![(1, vec![9, 1, 2])]),
			],
		)
		// Gain consensus on the final elections
		.expect_consensus_multi(vec![(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![81, 1, 93],
				None,
			),
			Some((vec![81, 1, 93], None)),
		)])
		// external chain doesn't move forward
		.test_on_finalize(
			&vec![None],
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections.ongoing.len(),
					1,
					"number of open elections should be 2"
				);
			},
			vec![
				// no new election created
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(0),
				// all elections have resolved now
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(0),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![(
					3,
					vec![81, 1, 93],
				)]),
			],
		);
}

#[test]
fn optimistic_election_result_saved_and_used_or_discarded_correctly() {
	const NUMBER_OF_ELECTIONS: ElectionCount = 2;

	const HEADERS_RECEIVED: [Header<Types>; NUMBER_OF_ELECTIONS as usize] = [
		Header { block_height: 11, hash: 11, parent_hash: 10 },
		Header { block_height: 12, hash: 12, parent_hash: 11 },
	];
	const NEXT_HEADER_RECEIVED: [Header<Types>; 1] =
		[Header { block_height: 13, hash: 13, parent_hash: 12 }];
	const LAST_HEADER_RECEIVED: [Header<Types>; 1] =
		[Header { block_height: 14, hash: 14, parent_hash: 13 }];

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN as u32,
			max_optimistic_elections: 1,
		})
		.build()
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				// Always + 1 since we always have an optimistic election going
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS.saturating_add(1) as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					NUMBER_OF_ELECTIONS.saturating_add(1),
				),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![
					(11, BWElectionType::ByHash(11)),
					(12, BWElectionType::ByHash(12)),
					(13, BWElectionType::Optimistic),
				]),
			],
		)
		.then(|| println!("We about to come to consensus on some blocks."))
		.expect_consensus_multi(vec![
			(
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![1],
					None,
				),
				Some((vec![1], None)),
			),
			(
				generate_votes(
					(0..40).collect(),
					Default::default(),
					Default::default(),
					vec![3, 4],
					None,
				),
				Some((vec![3, 4], None)),
			),
		])
		.test_on_finalize(
			&vec![None],
			|_| {
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2);
			},
			vec![
				// Only optimistic election is ongoing
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(2),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![(
					13,
					BWElectionType::Optimistic,
				)]),
			],
		)
		.then(|| println!("We're about to come to consensus on an optimistic block"))
		.expect_consensus(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![7],
				Some(13),
			),
			Some((vec![7], Some(13))),
		)
		// We reached consensus over the optimistic block, we will now receive the corresponding
		// header from the BHW
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: NEXT_HEADER_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				// Optimized block saved in cache processed correctly
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(1),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![(
					14,
					BWElectionType::Optimistic,
				)]),
			],
		)
		.expect_consensus(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![100],
				Some(140),
			),
			Some((vec![100], Some(140))),
		)
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: LAST_HEADER_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				// The last header doesn't match the optimistic block saved in cache, we discard it
				// and open a normal (by_hash) election for it
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
				// Optimized block not processed since its hash didn't match the hash of the
				// corresponding header received
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![]),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![
					(14, BWElectionType::ByHash(14)),
					(15, BWElectionType::Optimistic),
				]),
			],
		)
		.expect_consensus_multi(vec![(
			generate_votes(
				(0..40).collect(),
				Default::default(),
				Default::default(),
				vec![10],
				None,
			),
			Some((vec![10], None)),
		)])
		.test_on_finalize(
			&vec![],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(1),
				Check::<SimpleBlockWitnesser>::emitted_prewitness_events(vec![(14, vec![10])]),
				Check::<SimpleBlockWitnesser>::open_elections_type_is(vec![(
					15,
					BWElectionType::Optimistic,
				)]),
			],
		);
}

#[test]
fn with_safe_mode_enabled() {
	const HEADERS_RECEIVED: [Header<Types>; (MAX_CONCURRENT_ELECTIONS * 2) as usize] = [
		Header { block_height: 1, hash: 1, parent_hash: 0 },
		Header { block_height: 2, hash: 2, parent_hash: 1 },
		Header { block_height: 3, hash: 3, parent_hash: 2 },
		Header { block_height: 4, hash: 4, parent_hash: 3 },
		Header { block_height: 5, hash: 5, parent_hash: 4 },
		Header { block_height: 6, hash: 6, parent_hash: 5 },
		Header { block_height: 7, hash: 7, parent_hash: 6 },
		Header { block_height: 8, hash: 8, parent_hash: 7 },
		Header { block_height: 9, hash: 9, parent_hash: 8 },
		Header { block_height: 10, hash: 10, parent_hash: 9 },
	];

	let all_votes = (1..=5).map(|_| create_votes_expectation(vec![5, 6, 7])).collect::<Vec<_>>();

	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState {
			elections: ElectionTracker { highest_ever_ongoing_election: 7, ..Default::default() },
			safemode_enabled: MockHook::new(ConstantHook::new(SafeModeStatus::Enabled)),
			..Default::default()
		})
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_ongoing_elections: MAX_CONCURRENT_ELECTIONS,
			safety_margin: SAFETY_MARGIN as u32,
			max_optimistic_elections: 0,
		})
		.build()
		.test_on_finalize(
			&vec![Some(ChainProgress { headers: HEADERS_RECEIVED.into(), removed: None })],
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					MAX_CONCURRENT_ELECTIONS as u8,
				),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					MAX_CONCURRENT_ELECTIONS,
				),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(0),
			],
		)
		.then(|| println!("We about to come to consensus on some blocks."))
		.expect_consensus_multi(all_votes)
		.test_on_finalize(
			&vec![None],
			|_| {
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
					MAX_CONCURRENT_ELECTIONS,
				);
			},
			vec![
				// Only elections up to block 7 are open, hence 2 elections only since blocks from
				// 1 to 5 were already witnessed
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
				Check::<SimpleBlockWitnesser>::rules_hook_called_n_times_for_age_zero(5),
			],
		);
}
