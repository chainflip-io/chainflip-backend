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

use super::{
	mocks::{Check, TestSetup},
	register_checks,
};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes, ElectoralSystem},
	electoral_systems::{block_height_tracking::ChainProgress, block_witnesser::*},
};
use cf_chains::{mocks::MockEthereum, Chain};
use sp_std::collections::btree_set::BTreeSet;

thread_local! {
	pub static PROPERTIES_TO_RETURN: std::cell::RefCell<Properties> = const { std::cell::RefCell::new(BTreeSet::new()) };
	pub static GENERATE_ELECTION_HOOK_CALLED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
	pub static PROCESS_BLOCK_DATA_HOOK_CALLED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
	// the actual block data that process_block_data was called with.
	pub static PROCESS_BLOCK_DATA_CALLED_WITH: std::cell::RefCell<Vec<(ChainBlockNumber, BlockData)>> = const { std::cell::RefCell::new(vec![]) };
	// Flag to pass through block data:
	pub static PASS_THROUGH_BLOCK_DATA: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
	pub static PROCESS_BLOCK_DATA_TO_RETURN: std::cell::RefCell<Vec<(ChainBlockNumber, BlockData)>> = const { std::cell::RefCell::new(vec![]) };
}

pub type ChainBlockNumber = <MockEthereum as Chain>::ChainBlockNumber;
pub type ValidatorId = u16;

pub type BlockData = Vec<u8>;

struct MockGenerateElectionHook<ChainBlockNumber, Properties> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, Properties)>,
}

fn range_n(n: u64) -> std::ops::RangeInclusive<u64> {
	n..=n
}

pub type Properties = BTreeSet<u16>;

impl BlockElectionPropertiesGenerator<ChainBlockNumber, Properties>
	for MockGenerateElectionHook<ChainBlockNumber, Properties>
{
	fn generate_election_properties(_root_to_witness: ChainBlockNumber) -> Properties {
		GENERATE_ELECTION_HOOK_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
		// The properties are not important to the logic of the electoral system itself, so we can
		// return empty.
		BTreeSet::new()
	}
}

struct MockBlockProcessor<ChainBlockNumber, BlockData> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, BlockData)>,
}

impl MockBlockProcessor<ChainBlockNumber, BlockData> {
	pub fn set_block_data_to_return(block_data: Vec<(ChainBlockNumber, BlockData)>) {
		PASS_THROUGH_BLOCK_DATA.with(|pass_through| pass_through.set(false));
		PROCESS_BLOCK_DATA_TO_RETURN
			.with(|block_data_to_return| *block_data_to_return.borrow_mut() = block_data);
	}
}

impl ProcessBlockData<ChainBlockNumber, BlockData>
	for MockBlockProcessor<ChainBlockNumber, BlockData>
{
	// We need to do more here, like store some state and push back.
	fn process_block_data(
		// This isn't so important, in these tests, it's important for the implemenation of the
		// hooks. e.g. to determine a safety margin.
		_chain_block_number: ChainBlockNumber,
		earliest_unprocessed_block: ChainBlockNumber,
		block_data: Vec<(ChainBlockNumber, BlockData)>,
	) -> Vec<(ChainBlockNumber, BlockData)> {
		PROCESS_BLOCK_DATA_HOOK_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));

		PROCESS_BLOCK_DATA_CALLED_WITH
			.with(|old_block_data| *old_block_data.borrow_mut() = block_data.clone());

		if PASS_THROUGH_BLOCK_DATA.with(|pass_through| pass_through.get()) {
			println!("passing through block data");
			block_data
		} else {
			PROCESS_BLOCK_DATA_TO_RETURN
				.with(|block_data_to_return| block_data_to_return.borrow().clone())
		}

		// TODO: Think about if we need this check. It's not currently enforced in the traits, so
		// perhaps instead we should handle cases where the hook returns any set of properties. It
		// would usually be wrong to do so, but this ES doens't have to break as a result.
		// check that all blocks in block_data_to_retun are in block_data to ensure test consistency
		// block_data_to_return
		// 	.clone()
		// 	.into_iter()
		// 	.for_each(|(block_number, block_data_return)| {
		// 		if let Some(data) = block_data_return {
		// 			assert!(block_data_vec.contains(&(block_number, data)));
		// 		} else {
		// 			assert!(!block_data_vec.iter().any(|(number, _)| number == &block_number));
		// 		}
		// 	});
	}
}

// We need to provide a mock chain here... MockEthereum might be what we're after
type SimpleBlockWitnesser = BlockWitnesser<
	MockEthereum,
	BlockData,
	Properties,
	ValidatorId,
	MockBlockProcessor<ChainBlockNumber, BlockData>,
	MockGenerateElectionHook<ChainBlockNumber, Properties>,
>;

register_checks! {
	SimpleBlockWitnesser {
		generate_election_properties_called_n_times(_pre, _post, n: u8) {
			assert_eq!(GENERATE_ELECTION_HOOK_CALLED.with(|hook_called| hook_called.get()), n, "generate_election_properties should have been called {} times so far!", n);
		},
		number_of_open_elections_is(_pre, post, n: ElectionCount) {
			assert_eq!(post.unsynchronised_state.elections_open_for.len(), n as usize, "Number of open elections should be {}", n);
		},
		process_block_data_called_n_times(_pre, _post, n: u8) {
			assert_eq!(PROCESS_BLOCK_DATA_HOOK_CALLED.with(|hook_called| hook_called.get()), n, "process_block_data should have been called {} times so far!", n);
		},
		process_block_data_called_last_with(_pre, _post, block_data: Vec<(ChainBlockNumber, BlockData)>) {
			assert_eq!(PROCESS_BLOCK_DATA_CALLED_WITH.with(|old_block_data| old_block_data.borrow().clone()), block_data, "process_block_data should have been called with {:?}", block_data);
		},
		unprocessed_data_is(_pre, post, data: Vec<(ChainBlockNumber, BlockData)>) {
			assert_eq!(post.unsynchronised_state.unprocessed_data, data, "Unprocessed data should be {:?}", data);
		},
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
			.map(|v| ConsensusVote { vote: Some(((), correct_data.clone())), validator_id: v })
			.chain(incorrect_voters.clone().into_iter().map(|v| ConsensusVote {
				vote: Some(((), incorrect_data.clone())),
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
	Option<<SimpleBlockWitnesser as ElectoralSystem>::Consensus>,
) {
	(
		generate_votes(
			(0..20).collect(),
			Default::default(),
			Default::default(),
			consensus.clone(),
		),
		Some(consensus),
	)
}

const MAX_CONCURRENT_ELECTIONS: ElectionCount = 5;

// We start an election for a block and there is nothing there. The base case.
#[test]
fn no_block_data_success() {
	const NEXT_BLOCK_RECEIVED: ChainBlockNumber = 1;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState::default())
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
		})
		.build()
		.test_on_finalize(
			&ChainProgress::Continuous(range_n(NEXT_BLOCK_RECEIVED)),
			|_| {},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
			],
		)
		.expect_consensus(
			generate_votes((0..20).collect(), Default::default(), Default::default(), vec![]),
			Some(vec![]),
		)
		.test_on_finalize(
			&ChainProgress::Continuous(range_n(NEXT_BLOCK_RECEIVED)),
			|_| {},
			vec![
				// No extra calls
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
				// We should receive an empty block data, but still get the block number. This is
				// necessary so we can track the last chain block we've processed.
				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
					NEXT_BLOCK_RECEIVED,
					vec![],
				)]),
			],
		);
}

#[test]
fn creates_multiple_elections_below_maximum_when_required() {
	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
	const NUMBER_OF_ELECTIONS: ElectionCount = MAX_CONCURRENT_ELECTIONS - 1;
	TestSetup::<SimpleBlockWitnesser>::default()
		.with_unsynchronised_state(BlockWitnesserState::default())
		.with_unsynchronised_settings(BlockWitnesserSettings {
			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
		})
		.build()
		.test_on_finalize(
			// Process multiple elections, but still less than the maximum concurrent
			&ChainProgress::Continuous(range_n(
				INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64),
			)),
			|pre_state| {
				assert_eq!(pre_state.unsynchronised_state.elections_open_for.len(), 0);
			},
			vec![
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(4),
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
			&ChainProgress::None(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64)),
			|pre_state| {
				assert_eq!(
					pre_state.unsynchronised_state.elections_open_for.len(),
					NUMBER_OF_ELECTIONS as usize
				);
			},
			vec![
				// Still no extra elections created.
				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
					NUMBER_OF_ELECTIONS as u8,
				),
				// we should have resolved two elections
				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
			],
		);
}

// #[test]
// fn creates_multiple_elections_limited_by_maximum() {
// 	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
// 	const NUMBER_OF_ELECTIONS_REQUIRED: ElectionCount = MAX_CONCURRENT_ELECTIONS * 2;
// 	let consensus_resolutions: Vec<(
// 		ConsensusVotes<SimpleBlockWitnesser>,
// 		Option<<SimpleBlockWitnesser as ElectoralSystem>::Consensus>,
// 	)> = vec![
// 		create_votes_expectation(vec![]),
// 		create_votes_expectation(vec![1, 3, 4]),
// 		// no progress on external chain but on finalize called again
// 	];
// 	let number_of_resolved_elections = consensus_resolutions.len();
// 	TestSetup::<SimpleBlockWitnesser>::default()
// 		.with_unsynchronised_state(BlockWitnesserState {
// 			last_block_received: INIT_LAST_BLOCK_RECEIVED,
// 			..BlockWitnesserState::default()
// 		})
// 		.with_unsynchronised_settings(BlockWitnesserSettings {
// 			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
// 		})
// 		.build()
// 		.test_on_finalize(
// 			// Process multiple elections, but still elss than the maximum concurrent
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS_REQUIRED as u64)),
// 			|pre_state| {
// 				assert_eq!(pre_state.unsynchronised_state.open_elections, 0);
// 			},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8,
// 				),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
// 					MAX_CONCURRENT_ELECTIONS,
// 				),
// 			],
// 		)
// 		// Only resolve two of the elections. The last 3 are unresolved at this point. But
// 		// we now have space to start new elections.
// 		.expect_consensus_multi(consensus_resolutions)
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS_REQUIRED as u64)),
// 			|pre_state| {
// 				assert_eq!(pre_state.unsynchronised_state.open_elections, MAX_CONCURRENT_ELECTIONS);
// 			},
// 			vec![
// 				// Still no extra elections created.
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8 + number_of_resolved_elections as u8,
// 				),
// 				// we should have resolved two elections
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
// 					MAX_CONCURRENT_ELECTIONS,
// 				),
// 			],
// 		);
// }

// #[test]
// fn reorg_clears_on_going_elections_and_continues() {
// 	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 10;
// 	const NEXT_BLOCK_NUMBER: ChainBlockNumber =
// 		INIT_LAST_BLOCK_RECEIVED + MAX_CONCURRENT_ELECTIONS as u64;
// 	const REORG_LENGTH: ChainBlockNumber = 3;

// 	let all_votes = (INIT_LAST_BLOCK_RECEIVED + 1..=NEXT_BLOCK_NUMBER)
// 		.map(|_| create_votes_expectation(vec![5, 6, 7]))
// 		.collect::<Vec<_>>();

// 	// We have already emitted an election for `INIT_LAST_BLOCK_RECEIVED` (see TestSetup below), so
// 	// we add 1.
// 	let expected_unprocessed_data = (INIT_LAST_BLOCK_RECEIVED + 1..=NEXT_BLOCK_NUMBER)
// 		.map(|i| (i, vec![5, 6, 7]))
// 		.collect::<Vec<_>>();

// 	let mut block_after_reorg_block_unprocessed_data = expected_unprocessed_data.clone();
// 	block_after_reorg_block_unprocessed_data
// 		.push(((NEXT_BLOCK_NUMBER - REORG_LENGTH), vec![5, 6, 77]));

// 	TestSetup::<SimpleBlockWitnesser>::default()
// 		.with_unsynchronised_state(BlockWitnesserState {
// 			last_block_received: INIT_LAST_BLOCK_RECEIVED,
// 			last_block_election_emitted_for: INIT_LAST_BLOCK_RECEIVED,
// 			..BlockWitnesserState::default()
// 		})
// 		.with_unsynchronised_settings(BlockWitnesserSettings {
// 			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
// 		})
// 		.build()
// 		.test_on_finalize(
// 			&range_n(NEXT_BLOCK_NUMBER),
// 			|_| {},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8,
// 				),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(
// 					MAX_CONCURRENT_ELECTIONS,
// 				),
// 				// No reorg, so we try processing any unprocessed state (there would be none at
// 				// this point though, since no elections have resolved).
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
// 			],
// 		)
// 		.expect_consensus_multi(all_votes)
// 		// Process votes as normal, storing the state
// 		.test_on_finalize(
// 			&range_n(NEXT_BLOCK_NUMBER + 1),
// 			|_| {},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8 + 1,
// 				),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(
// 					expected_unprocessed_data.clone(),
// 				),
// 			],
// 		)
// 		// Reorg occurs
// 		.test_on_finalize(
// 			&range_n(NEXT_BLOCK_NUMBER - REORG_LENGTH),
// 			|_| {},
// 			// We remove the actives ones and open one for the first block that we detected a
// 			// reorg for.
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8 + 2,
// 				),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				// There was a reorg, so there's definitely nothing to process since we're deleting
// 				// all the data and just starting a new election, no extra calls here.
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
// 				// We keep the data, since it may need to be used by process_block_data to
// 				// deduplicate actions. We don't want to submit an action twice.
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(expected_unprocessed_data),
// 			],
// 		)
// 		.expect_consensus_multi(vec![create_votes_expectation(vec![5, 6, 77])])
// 		.test_on_finalize(
// 			&range_n((NEXT_BLOCK_NUMBER - REORG_LENGTH) + 1),
// 			|_| {},
// 			// We remove the actives ones and open one for the first block that we detected a
// 			// reorg for.
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					MAX_CONCURRENT_ELECTIONS as u8 + 3,
// 				),
// 				// We resolve one, but we've also progressed, so we open one.
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(3),
// 				// We now have two pieces of data for the same block.
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(
// 					block_after_reorg_block_unprocessed_data,
// 				),
// 			],
// 		);
// }

// #[test]
// fn partially_processed_block_data_processed_next_on_finalize() {
// 	let first_block_consensus: BlockData = vec![5, 6, 7];

// 	let first_block_data_after_processing: Vec<_> =
// 		first_block_consensus.clone().into_iter().take(2).collect();

// 	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
// 	TestSetup::<SimpleBlockWitnesser>::default()
// 		.with_unsynchronised_state(BlockWitnesserState {
// 			last_block_received: INIT_LAST_BLOCK_RECEIVED,
// 			..BlockWitnesserState::default()
// 		})
// 		.with_unsynchronised_settings(BlockWitnesserSettings {
// 			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
// 		})
// 		.build()
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + 1),
// 			|_| {},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(1),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(1),
// 				// We haven't come to consensus on any elections, so there's no unprocessed data.
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![]),
// 			],
// 		)
// 		.expect_consensus_multi(vec![create_votes_expectation(first_block_consensus.clone())])
// 		.then(|| {
// 			// We process one of the items, so we return only 2 of 3.
// 			MockBlockProcessor::set_block_data_to_return(vec![(
// 				INIT_LAST_BLOCK_RECEIVED + 1,
// 				first_block_data_after_processing.clone(),
// 			)]);
// 		})
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + 2),
// 			|_| {},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
// 				// One opened, one closed.
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				// We call it again.
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(2),
// 				// We have the election data for the election we emitted before now. We try to
// 				// process it.
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
// 					INIT_LAST_BLOCK_RECEIVED + 1,
// 					first_block_consensus,
// 				)]),
// 			],
// 		)
// 		// No progress on external chain, so state should be the same as above, except that we
// 		// processed one of the items last time.
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + 2),
// 			|_| {},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(2),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(1),
// 				// We call it again.
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_n_times(3),
// 				Check::<SimpleBlockWitnesser>::process_block_data_called_last_with(vec![(
// 					INIT_LAST_BLOCK_RECEIVED + 1,
// 					first_block_data_after_processing,
// 				)]),
// 			],
// 		);
// }

// #[test]
// fn elections_resolved_out_of_order_has_no_impact() {
// 	const INIT_LAST_BLOCK_RECEIVED: ChainBlockNumber = 0;
// 	const FIRST_ELECTION_BLOCK_CREATED: ChainBlockNumber = INIT_LAST_BLOCK_RECEIVED + 1;
// 	const SECOND_ELECTION_BLOCK_CREATED: ChainBlockNumber = FIRST_ELECTION_BLOCK_CREATED + 1;
// 	const NUMBER_OF_ELECTIONS: ElectionCount = 2;
// 	TestSetup::<SimpleBlockWitnesser>::default()
// 		.with_unsynchronised_state(BlockWitnesserState {
// 			last_block_received: INIT_LAST_BLOCK_RECEIVED,
// 			..BlockWitnesserState::default()
// 		})
// 		.with_unsynchronised_settings(BlockWitnesserSettings {
// 			max_concurrent_elections: MAX_CONCURRENT_ELECTIONS,
// 		})
// 		.build()
// 		.test_on_finalize(
// 			// Process multiple elections, but still elss than the maximum concurrent
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + 2),
// 			|pre_state| {
// 				assert_eq!(pre_state.unsynchronised_state.open_elections, 0);
// 			},
// 			vec![
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					NUMBER_OF_ELECTIONS as u8,
// 				),
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(NUMBER_OF_ELECTIONS),
// 			],
// 		)
// 		.expect_consensus_multi(vec![
// 			(
// 				// no consensus
// 				generate_votes((0..20).collect(), (0..20).collect(), Default::default(), vec![]),
// 				None,
// 			),
// 			(
// 				// consensus
// 				generate_votes(
// 					(0..40).collect(),
// 					Default::default(),
// 					Default::default(),
// 					vec![1, 3, 4],
// 				),
// 				Some(vec![1, 3, 4]),
// 			),
// 		])
// 		// no progress on external chain but on finalize called again
// 		// TODO: Check the new elections have kicked off correct
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64) + 1),
// 			|pre_state| {
// 				assert_eq!(pre_state.unsynchronised_state.open_elections, NUMBER_OF_ELECTIONS);
// 			},
// 			vec![
// 				// one extra election created
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					(NUMBER_OF_ELECTIONS + 1) as u8,
// 				),
// 				// we should have resolved one election, and started one election
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![(
// 					SECOND_ELECTION_BLOCK_CREATED,
// 					vec![1, 3, 4],
// 				)]),
// 			],
// 		)
// 		// gain consensus on the first emitted election now
// 		.expect_consensus_multi(vec![(
// 			generate_votes(
// 				(0..40).collect(),
// 				Default::default(),
// 				Default::default(),
// 				vec![9, 1, 2],
// 			),
// 			Some(vec![9, 1, 2]),
// 		)])
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64) + 2),
// 			|pre_state| {
// 				assert_eq!(
// 					pre_state.unsynchronised_state.open_elections, 2,
// 					"number of open elections should be 2"
// 				);
// 			},
// 			vec![
// 				// one extra election created
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					(NUMBER_OF_ELECTIONS + 2) as u8,
// 				),
// 				// we should have resolved one elections, and started one election
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(2),
// 				// Now the first election we emitted is resolved, and its block data should be
// 				// stored, and we should still have the second election block data.
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![
// 					(SECOND_ELECTION_BLOCK_CREATED, vec![1, 3, 4]),
// 					(FIRST_ELECTION_BLOCK_CREATED, vec![9, 1, 2]),
// 				]),
// 			],
// 		)
// 		// Gain consensus on the final elections
// 		.expect_consensus_multi(vec![
// 			(
// 				generate_votes(
// 					(0..40).collect(),
// 					Default::default(),
// 					Default::default(),
// 					vec![81, 1, 93],
// 				),
// 				Some(vec![81, 1, 93]),
// 			),
// 			(
// 				generate_votes(
// 					(0..40).collect(),
// 					Default::default(),
// 					Default::default(),
// 					vec![69, 69, 69],
// 				),
// 				Some(vec![69, 69, 69]),
// 			),
// 		])
// 		// external chain doesn't move forward
// 		.test_on_finalize(
// 			&range_n(INIT_LAST_BLOCK_RECEIVED + (NUMBER_OF_ELECTIONS as u64) + 2),
// 			|pre_state| {
// 				assert_eq!(
// 					pre_state.unsynchronised_state.open_elections, 2,
// 					"number of open elections should be 2"
// 				);
// 			},
// 			vec![
// 				// one extra election created
// 				Check::<SimpleBlockWitnesser>::generate_election_properties_called_n_times(
// 					(NUMBER_OF_ELECTIONS + 2) as u8,
// 				),
// 				// all elections have resolved now
// 				Check::<SimpleBlockWitnesser>::number_of_open_elections_is(0),
// 				// Now the last two elections are resolved in order
// 				Check::<SimpleBlockWitnesser>::unprocessed_data_is(vec![
// 					(SECOND_ELECTION_BLOCK_CREATED, vec![1, 3, 4]),
// 					(FIRST_ELECTION_BLOCK_CREATED, vec![9, 1, 2]),
// 					(SECOND_ELECTION_BLOCK_CREATED + 1, vec![81, 1, 93]),
// 					(SECOND_ELECTION_BLOCK_CREATED + 2, vec![69, 69, 69]),
// 				]),
// 			],
// 		);
// }
