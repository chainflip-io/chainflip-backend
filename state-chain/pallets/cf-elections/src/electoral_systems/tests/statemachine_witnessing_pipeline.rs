//! This file tests the BlockWitnesser and the BlockHeightWitnesser state machines composed together
//! on realistic inputs of a chain with many reorgs.

pub mod chainstate_simulation;

use itertools::Either;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
use sp_std::{fmt::Debug, vec::Vec};
use std::collections::{BTreeSet, VecDeque};

use crate::electoral_systems::{
	block_height_witnesser::{
		primitives::NonemptyContinuousHeaders,
		state_machine::{BHWPhase, BlockHeightWitnesser},
		BHWTypes, ChainBlockNumberOf,
	},
	block_witnesser::{
		block_processor::{tests::MockBtcEvent, BlockProcessor, BlockProcessorEvent},
		primitives::{ElectionTrackerEvent, SafeModeStatus},
		state_machine::{
			BWStatemachine, BWTypes, BlockWitnesserSettings, BlockWitnesserState,
			EngineElectionType,
		},
	},
	state_machine::{
		core::TypesFor,
		state_machine::{AbstractApi, InputOf, Statemachine},
		test_utils::BTreeMultiSet,
	},
};
use cf_traits::{hook_test_utils::MockHook, Validate};
use chainstate_simulation::*;

macro_rules! try_get {
    ($($tt:tt)+) => {
        |x| match x {$($tt)+(x) => Some(x), _ => None}
    };
}

#[expect(clippy::type_complexity)]
pub trait AbstractVoter<M: Statemachine> {
	fn vote(
		&mut self,
		index: Vec<M::Query>,
	) -> Option<Vec<Either<M::Context, (M::Query, M::Response)>>>;
}

type Event = String;
type Types = TypesFor<(u8, u32, Vec<Event>)>;

type BW = BWStatemachine<Types>;
#[expect(clippy::upper_case_acronyms)]
type BHW = BlockHeightWitnesser<Types>;

const OFFSET: usize = 20;

impl AbstractVoter<BHW> for FlatChainProgression<Event> {
	fn vote(&mut self, indices: Vec<<BHW as AbstractApi>::Query>) -> Option<Vec<InputOf<BHW>>> {
		let chain = MockChain::new_with_offset(OFFSET, self.get_next_chain()?);

		let mut result = Vec::new();

		for index in indices {
			let best_block = chain.get_best_block_header();
			if best_block.block_height < index.witness_from_index {
				continue;
			}

			let bhw_input = if index.witness_from_index == 0 {
				NonemptyContinuousHeaders::try_new(VecDeque::from([best_block])).unwrap()
			} else {
				let headers = (index.witness_from_index..=chain.get_best_block_height())
					.map(|height| chain.get_block_header(height));
				if headers.len() == 0 {
					continue;
				}
				if let Some(headers) = headers.into_iter().collect::<Option<Vec<_>>>() {
					NonemptyContinuousHeaders::try_new(VecDeque::from_iter(headers)).unwrap()
				} else {
					continue
				}
			};

			result.push(Either::Right((index, bhw_input)));
		}

		Some(result)
	}
}

impl AbstractVoter<BW> for FlatChainProgression<String> {
	fn vote(&mut self, indices: Vec<<BW as AbstractApi>::Query>) -> Option<Vec<InputOf<BW>>> {
		let mut inputs = Vec::new();
		for index in indices {
			let chain = MockChain::<String, Types>::new_with_offset(OFFSET, self.get_next_chain()?);

			match index.election_type {
				EngineElectionType::BlockHeight { submit_hash } => {
					if let Some(block_data) = chain.get_block_by_height(index.block_height) {
						let header = chain.get_block_header(index.block_height).unwrap();
						inputs.push(Either::Right((
							index,
							(
								block_data.into_iter().collect(),
								if submit_hash { Some(header.hash) } else { None },
							),
						)));
					}
				},
				EngineElectionType::ByHash(hash) => {
					if let Some(block_data) = chain.get_block_by_hash(hash) {
						inputs
							.push(Either::Right((index, (block_data.into_iter().collect(), None))));
					}
				},
			}
		}
		Some(inputs)
	}
}

#[cfg(test)]
fn run_simulation(blocks: ForkedFilledChain) {
	let mut chains = blocks_into_chain_progression(&blocks.blocks);

	const SAFETY_MARGIN: u32 = 8;
	const SAFETY_BUFFER: u32 = 16;

	// get final chain so we can check that we emitted the correct events:
	let final_chain = chains.get_final_chain();
	let finalized_blocks: Vec<_> = final_chain;
	let finalized_events: BTreeSet<_> =
		finalized_blocks.iter().flat_map(|block| block.events.iter()).collect();

	// prepare the state machines
	let mut bhw_state: BlockHeightWitnesser<Types> = BlockHeightWitnesser {
		phase: BHWPhase::Starting,
		block_height_update: MockHook::default(),
		on_reorg: MockHook::default(),
	};
	let bhw_settings: BlockHeightWitnesserSettings =
		BlockHeightWitnesserSettings { safety_buffer: SAFETY_BUFFER };
	let block_processor: BlockProcessor<Types> = BlockProcessor {
		blocks_data: Default::default(),
		processed_events: Default::default(),
		rules: Default::default(),
		execute: MockHook::default(),
		debug_events: MockHook::default(),
	};
	let mut bw_state: BlockWitnesserState<Types> = BlockWitnesserState {
		elections: Default::default(),
		generate_election_properties_hook: Default::default(),
		safemode_enabled: MockHook::new(ConstantHook::new(SafeModeStatus::Disabled)),
		processed_up_to: Default::default(),
		block_processor,
	};
	let bw_settings = BlockWitnesserSettings {
		max_ongoing_elections: 4,
		safety_margin: SAFETY_MARGIN,
		safety_buffer: SAFETY_BUFFER,
		max_optimistic_elections: 1,
	};

	#[derive(Clone, Debug)]
	enum BWTrace<T: BWTypes, T0: BHWTypes> {
		Input(InputOf<BWStatemachine<T>>),
		InputBHW(InputOf<BlockHeightWitnesser<T0>>),
		#[expect(dead_code)]
		Output(Vec<(ChainBlockNumberOf<T::Chain>, T::Event)>),
		Event(BlockProcessorEvent<T>),
		ET(ElectionTrackerEvent<T>),
	}

	let mut history = Vec::new();
	let mut total_outputs = Vec::new();

	let print_bw_history = |bw_history: &Vec<BWTrace<Types, Types>>| {
		bw_history
			.iter()
			.map(|event| format!("{event:?}"))
			.intersperse("\n".to_string())
			.collect::<String>()
	};

	while chains.has_chains() {
		// run BHW
		let bhw_outputs = if let Some(inputs) =
			AbstractVoter::<BHW>::vote(&mut chains, BHW::get_queries(&mut bhw_state))
		{
			let mut outputs = Vec::new();
			for input in inputs {
				// ensure that input is correct
				BHW::validate_input(&mut bhw_state, &input).unwrap();

				history.push(BWTrace::InputBHW(input.clone()));

				let output =
					BHW::step(&mut bhw_state, input, &bhw_settings).unwrap_or_else(|err| {
						panic!("{err:?}, BHW failed with history: {history:?} and state: {bhw_state:#?}")
					});

				outputs.push(output);
			}
			outputs
		} else {
			break
		};

		// ---- BW ----

		let mut bw_outputs = if let Some(inputs) =
			AbstractVoter::<BW>::vote(&mut chains, BW::get_queries(&mut bw_state))
		{
			let mut outputs = Vec::new();

			// run BW on BHW outputs (context)
			for bhw_output in bhw_outputs {
				history.push(BWTrace::Input(Either::Left(bhw_output.clone())));

				bw_state.elections.is_valid().unwrap_or_else(|err| {
					panic!(
						"{err:?}, BW failed with history: {} and state: {bw_state:#?}",
						print_bw_history(&history)
					)
				});

				BW::step_and_validate(&mut bw_state, Either::Left(bhw_output), &bw_settings)
					.unwrap();

				history.extend(
					bw_state.elections.debug_events.take_history().into_iter().map(BWTrace::ET),
				);

				history.extend(
					bw_state
						.block_processor
						.debug_events
						.take_history()
						.into_iter()
						.map(BWTrace::Event),
				);

				let mut output = bw_state.block_processor.execute.take_history();
				history.extend(output.iter().cloned().map(BWTrace::Output));

				outputs.append(&mut output);
			}

			// run on BW inputs (consensus)
			for input in inputs {
				history.push(BWTrace::Input(input.clone()));

				bw_state.elections.is_valid().unwrap_or_else(|err| {
					panic!(
						"{err:?}, BW failed with history: {} and state: {bw_state:#?}",
						print_bw_history(&history)
					)
				});

				BW::step_and_validate(&mut bw_state, input, &bw_settings).unwrap();

				history.extend(
					bw_state.elections.debug_events.take_history().into_iter().map(BWTrace::ET),
				);

				history.extend(
					bw_state
						.block_processor
						.debug_events
						.take_history()
						.into_iter()
						.map(BWTrace::Event),
				);

				let mut output = bw_state.block_processor.execute.take_history();
				history.extend(output.iter().cloned().map(BWTrace::Output));

				outputs.append(&mut output);
			}
			outputs
		} else {
			break
		};

		total_outputs.append(&mut bw_outputs);
	}

	use std::fmt::Write;

	use crate::electoral_systems::block_height_witnesser::BlockHeightWitnesserSettings;
	use cf_traits::hook_test_utils::ConstantHook;
	let mut printed: String = Default::default();
	for output in total_outputs.clone() {
		if output.len() == 0 {
			write!(printed, "No events").unwrap();
		}
		for (height, event) in output {
			let event = match event {
				MockBtcEvent::PreWitness(data) => format!("Pre {}", data),
				MockBtcEvent::Witness(data) => format!("Wit {}", data),
			};
			write!(printed, "{height}: {}, ", event).unwrap();
		}
		writeln!(printed).unwrap();
	}

	let counted_events: BTreeMultiSet<(u8, MockBtcEvent<Event>)> =
		total_outputs.into_iter().flatten().collect();

	// verify that each event was emitted only one time
	for (event, count) in counted_events.0.clone() {
		if count > 1 {
			panic!("Got event {event:?} in total {count} times           events: {printed}              bw_input_history: {}",
                print_bw_history(&history)
            );
		}
	}

	// ensure that we only emit witness events that are on the final chain
	let emitted_witness_events: BTreeSet<_> = counted_events
		.0
		.keys()
		.map(|(_, b)| b)
		.filter_map(try_get!(MockBtcEvent::Witness))
		.cloned()
		.collect();
	let expected_witness_events: BTreeSet<_> = finalized_events.into_iter().cloned().collect();
	assert_eq!(emitted_witness_events, expected_witness_events,
            "got witness events: {emitted_witness_events:?}, expected_witness_events: {expected_witness_events:?}, bw_input_history: {}",
            history.iter().map(|event| format!("{event:?}")).intersperse("\n".to_string()).collect::<String>()
        );
}

/// Generates random chain progressions and simulates running the witnessing electoral systems with
/// this input.
#[test]
pub fn test_all() {
	let mut runner = TestRunner::new(Config {
		source_file: Some(file!()),
		// Default is: 256
		// Value useful for development is: 256 * 60
		// Value for quick CI: 100
		cases: 256,
		failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
			"proptest-regressions-full-pipeline",
		))),
		..Default::default()
	});
	runner
		.run(&generate_blocks_with_tail(), |blocks| {
			run_simulation(blocks);
			Ok(())
		})
		.unwrap();
}

/// This runs the witnessing against the case where there's a reorg and a "replacement block"
/// doesn't arrive within SAFETY_BUFFER blocks. This case wasn't handled correctly previously,
/// discovered in PRO-2298 and fixed in PRO-2299.
#[test]
fn test_delayed_election_result_after_reorg_is_handled() {
	let mut blocks = vec![
		ForkedBlock::Block(FilledBlock {
			block_id: 0,
			data: vec![],
			data_delays: vec![0],
			resolution_delay: 0,
		}),
		ForkedBlock::Fork(vec![ForkedBlock::Block(FilledBlock {
			block_id: 1,
			data: vec!["b".to_string(), "c".into(), "d".into()],
			data_delays: vec![0, 0],
			resolution_delay: 0,
		})]),
		ForkedBlock::Block(FilledBlock {
			block_id: 2,
			data: vec!["a".into(), "b".into(), "c".into()],
			data_delays: vec![1],
			resolution_delay: 21,
		}),
	];

	for _ in 0..25 {
		blocks.push(ForkedBlock::Block(FilledBlock {
			block_id: 3,
			data: vec![],
			data_delays: vec![0, 0, 0, 0, 0],
			resolution_delay: 0,
		}));
	}

	run_simulation(ForkedFilledChain { blocks });
}

/// A fork is reorged away and replaced by a shorter chain.
///
/// This test case is covered by proptesting if `max_data_delay` >= 4
/// and `time_steps_per_block` contains 0..=8.
#[test]
fn test_reorg_into_shorter_chain() {
	let mut blocks = vec![
		ForkedBlock::Block(FilledBlock {
			block_id: 0,
			data: vec![],
			data_delays: vec![0],
			resolution_delay: 0,
		}),
		ForkedBlock::Fork(vec![ForkedBlock::Block(FilledBlock {
			block_id: 1,
			data: vec!["b".to_string(), "c".into(), "d".into()],
			data_delays: vec![0, 0, 0, 0, 0, 0],
			resolution_delay: 0,
		})]),
		ForkedBlock::Block(FilledBlock {
			block_id: 2,
			data: vec!["a".into(), "b".into(), "c".into()],
			data_delays: vec![],
			resolution_delay: 0,
		}),
		ForkedBlock::Block(FilledBlock {
			block_id: 3,
			data: vec![],
			// These data delays have the following effect:
			// when the time of the chain that ends at this block comes,
			// it is queried 2 times with delay 0, that is, it actually
			// returns this state of the chain. Then after that, for 5 queries,
			// it has delay 3, i.e., it will return the state of the chain
			// at block_id: 0. That's a 1 block chain.
			//
			// So effectively we create the following scenario:
			// post-fork, the BHW queries the highest block, gets block_id 3 at height 2 with
			// parent block_id 2 at height 1. It currently knows that block_id 1 should
			// be at height 1. This means it recognized that we have a reorg at our hands.
			//
			// Then it forces a requerying of all known blocks. Since we set the delay
			// to 3, this means that the engines will get a chain that's 3 steps in the past,
			// Which is in our case the chain that only contains block_id 0.
			//
			// This means that the BHW will get a merge_info with added = [], and removed =
			// {[block_id 1, height 1]}
			//
			// This was a critical case that broke the BHW, because it didn't forward the reorg
			// information in case of `added` being empty.
			data_delays: vec![0, 0, 3, 3, 3, 3, 3, 3],
			resolution_delay: 0,
		}),
	];

	for _ in 0..25 {
		blocks.push(ForkedBlock::Block(FilledBlock {
			block_id: 4,
			data: vec![],
			data_delays: vec![0, 0, 0, 0, 0],
			resolution_delay: 0,
		}));
	}

	run_simulation(ForkedFilledChain { blocks });
}
