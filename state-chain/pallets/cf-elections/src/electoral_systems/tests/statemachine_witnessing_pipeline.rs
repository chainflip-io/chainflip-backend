//! This file tests the BlockWitnesser and the BlockHeightWitnesser state machines composed together
//! on realistic inputs of a chain with many reorgs.

pub mod chainstate_simulation;

use cf_chains::witness_period::{BlockWitnessRange, SaturatingStep};
use chainstate_simulation::*;
use itertools::Either;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
use sp_std::{fmt::Debug, vec::Vec};
use std::collections::{BTreeSet, VecDeque};

use crate::electoral_systems::{
	block_height_witnesser::{
		primitives::NonemptyContinuousHeaders,
		state_machine::{BHWPhase, BlockHeightWitnesser},
		BHWTypes, ChainBlockNumberOf, ChainTypes,
	},
	block_witnesser::{
		block_processor::{tests::MockBtcEvent, BlockProcessor, BlockProcessorEvent},
		primitives::ElectionTrackerEvent,
		state_machine::{
			BWProcessorTypes, BWStatemachine, BWTypes, BlockWitnesserSettings, BlockWitnesserState,
			DebugEventHook, ElectionTrackerDebugEventHook, EngineElectionType, ExecuteHook,
			HookTypeFor,
		},
	},
	state_machine::{
		core::{hook_test_utils::MockHook, Hook, HookType, TypesFor, Validate},
		state_machine::{AbstractApi, InputOf, Statemachine},
		test_utils::BTreeMultiSet,
	},
};

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

trait HistoryHook<T: HookType> {
	fn take_history(&mut self) -> Vec<T::Input>;
}

impl<T: HookType, const NAME: &'static str, WrappedHook: Hook<T>> HistoryHook<T>
	for MockHook<T, NAME, WrappedHook>
{
	fn take_history(&mut self) -> Vec<T::Input> {
		MockHook::take_history(self)
	}
}

type Event = String;

// ============================================================================
// Type aliases for regular block numbers (WITNESS_RANGE = 1, like Ethereum)
// ============================================================================
type Types = TypesFor<(u8, u32, Vec<Event>)>;

// ============================================================================
// Type aliases for BlockWitnessRange (WITNESS_RANGE = 24, like Arbitrum)
// ============================================================================
type RangedTypes = TypesFor<(BlockWitnessRange<RangedWitnessConfig>, BlockId, Vec<Event>)>;

const OFFSET: usize = 20;

// ============================================================================
// AbstractVoter implementations - generic over the chain types
// ============================================================================

/// Generic AbstractVoter implementation for BlockHeightWitnesser
impl<T> AbstractVoter<BlockHeightWitnesser<T>> for FlatChainProgression<Event>
where
	T: BHWTypes,
	T::Chain: ChainTypes<ChainBlockHash = BlockId>,
	ChainBlockNumberOf<T::Chain>: Default + Ord + Clone + MockChainBlockNumber,
{
	fn vote(
		&mut self,
		indices: Vec<<BlockHeightWitnesser<T> as AbstractApi>::Query>,
	) -> Option<Vec<InputOf<BlockHeightWitnesser<T>>>> {
		let chain = MockChain::<Event, T::Chain>::new_with_offset(OFFSET, self.get_next_chain()?);

		let mut result = Vec::new();

		for index in indices {
			let best_block_height = chain.get_best_block_height();
			let Some(best_block) = chain.get_block_header(best_block_height) else {
				continue;
			};
			if best_block.block_height < index.witness_from_index {
				continue;
			}

			let bhw_input = if index.witness_from_index == Default::default() {
				NonemptyContinuousHeaders::try_new(VecDeque::from([best_block])).unwrap()
			} else {
				// Check if range is empty by comparing start and end
				if index.witness_from_index > best_block_height {
					continue;
				}
				let mut headers = Vec::new();
				let mut missing_header = false;
				for height in index.witness_from_index..=best_block_height {
					match chain.get_block_header(height) {
						Some(header) => headers.push(header),
						None => {
							missing_header = true;
							break;
						},
					}
				}
				if missing_header {
					continue;
				}
				if headers.is_empty() {
					continue;
				}
				NonemptyContinuousHeaders::try_new(VecDeque::from_iter(headers)).unwrap()
			};

			result.push(Either::Right((index, bhw_input)));
		}

		Some(result)
	}
}

/// Generic AbstractVoter implementation for BlockWitnesser
impl<T> AbstractVoter<BWStatemachine<T>> for FlatChainProgression<String>
where
	T: BWTypes<BlockData = Vec<Event>>,
	T::Chain: ChainTypes<ChainBlockHash = BlockId>,
	ChainBlockNumberOf<T::Chain>: Default + Ord + Clone + MockChainBlockNumber,
{
	fn vote(
		&mut self,
		indices: Vec<<BWStatemachine<T> as AbstractApi>::Query>,
	) -> Option<Vec<InputOf<BWStatemachine<T>>>> {
		let mut inputs = Vec::new();
		for index in indices {
			let chain =
				MockChain::<String, T::Chain>::new_with_offset(OFFSET, self.get_next_chain()?);

			match index.election_type {
				EngineElectionType::BlockHeight { submit_hash } => {
					if let Some(block_data) = chain.get_block_by_height(index.block_height) {
						let header = chain.get_block_header(index.block_height).unwrap();
						inputs.push(Either::Right((
							index,
							(block_data, if submit_hash { Some(header.hash) } else { None }),
						)));
					}
				},
				EngineElectionType::ByHash(hash) => {
					if let Some(block_data) = chain.get_block_by_hash(hash) {
						inputs.push(Either::Right((index, (block_data, None))));
					}
				},
			}
		}
		Some(inputs)
	}
}

// ============================================================================
// Generic simulation function
// ============================================================================

#[cfg(test)]
fn run_simulation_generic<T>(blocks: ForkedFilledChain)
where
	T: BWTypes<Chain = <T as BHWTypes>::Chain, BlockData = Vec<Event>, Event = MockBtcEvent<Event>>
		+ BHWTypes
		+ Default,
	<T as BHWTypes>::Chain: ChainTypes<ChainBlockHash = BlockId>,
	<T as BWProcessorTypes>::Chain: ChainTypes<ChainBlockHash = BlockId>,
	ChainBlockNumberOf<<T as BHWTypes>::Chain>: Default + Ord + Clone + Debug + SaturatingStep,
	<T as BHWTypes>::BlockHeightChangeHook: Default,
	<T as BHWTypes>::ReorgHook: Default,
	<T as BWTypes>::ElectionPropertiesHook: Default + Send,
	<T as BWTypes>::SafeModeEnabledHook: Default + Send,
	<T as BWTypes>::ProcessedUpToHook: Default + Send,
	<T as BWTypes>::ElectionTrackerDebugEventHook:
		Default + Send + HistoryHook<HookTypeFor<T, ElectionTrackerDebugEventHook>>,
	<T as BWProcessorTypes>::Rules: Send,
	<T as BWProcessorTypes>::Execute: Send + HistoryHook<HookTypeFor<T, ExecuteHook>>,
	<T as BWProcessorTypes>::DebugEventHook: Send + HistoryHook<HookTypeFor<T, DebugEventHook>>,
	FlatChainProgression<Event>: AbstractVoter<BlockHeightWitnesser<T>>,
	FlatChainProgression<String>: AbstractVoter<BWStatemachine<T>>,
	BlockProcessor<T>: Default,
	BlockWitnesserState<T>: Default,
{
	use crate::electoral_systems::block_height_witnesser::BlockHeightWitnesserSettings;
	use std::fmt::Write;

	let mut chains = blocks_into_chain_progression(&blocks.blocks);

	const SAFETY_MARGIN: u32 = 8;
	const SAFETY_BUFFER: u32 = 16;

	// get final chain so we can check that we emitted the correct events:
	let final_chain = chains.get_final_chain();
	let finalized_blocks: Vec<_> = final_chain;
	let finalized_events: BTreeSet<_> =
		finalized_blocks.iter().flat_map(|block| block.events.iter()).collect();

	// prepare the state machines
	let mut bhw_state: BlockHeightWitnesser<T> = BlockHeightWitnesser {
		phase: BHWPhase::Starting,
		block_height_update: Default::default(),
		on_reorg: Default::default(),
	};
	let bhw_settings: BlockHeightWitnesserSettings =
		BlockHeightWitnesserSettings { safety_buffer: SAFETY_BUFFER };
	let block_processor: BlockProcessor<T> = BlockProcessor::default();
	let mut bw_state: BlockWitnesserState<T> = BlockWitnesserState {
		elections: Default::default(),
		generate_election_properties_hook: Default::default(),
		safemode_enabled: Default::default(),
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
	enum BWTrace<T0: BWTypes, T1: BHWTypes> {
		Input(InputOf<BWStatemachine<T0>>),
		InputBHW(InputOf<BlockHeightWitnesser<T1>>),
		#[expect(dead_code)]
		Output(Vec<(ChainBlockNumberOf<T0::Chain>, T0::Event)>),
		Event(BlockProcessorEvent<T0>),
		ET(ElectionTrackerEvent<T0>),
	}

	let mut history: Vec<BWTrace<T, T>> = Vec::new();
	#[expect(clippy::type_complexity)]
	let mut total_outputs: Vec<
		Vec<(ChainBlockNumberOf<<T as BWProcessorTypes>::Chain>, T::Event)>,
	> = Vec::new();

	let print_bw_history = |bw_history: &Vec<BWTrace<T, T>>| {
		bw_history
			.iter()
			.map(|event| format!("{event:?}"))
			.intersperse("\n".to_string())
			.collect::<String>()
	};

	while chains.has_chains() {
		// run BHW
		let bhw_outputs = if let Some(inputs) = AbstractVoter::<BlockHeightWitnesser<T>>::vote(
			&mut chains,
			BlockHeightWitnesser::<T>::get_queries(&mut bhw_state),
		) {
			let mut outputs = Vec::new();
			for input in inputs {
				// ensure that input is correct
				BlockHeightWitnesser::<T>::validate_input(&mut bhw_state, &input).unwrap();

				history.push(BWTrace::InputBHW(input.clone()));

				let output = BlockHeightWitnesser::<T>::step(&mut bhw_state, input, &bhw_settings)
					.unwrap_or_else(|err| {
						panic!(
							"{err:?}, BHW failed with history: {history:?} and state: {bhw_state:#?}"
						)
					});

				outputs.push(output);
			}
			outputs
		} else {
			break
		};

		// ---- BW ----

		let mut bw_outputs = if let Some(inputs) = AbstractVoter::<BWStatemachine<T>>::vote(
			&mut chains,
			BWStatemachine::<T>::get_queries(&mut bw_state),
		) {
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

				BWStatemachine::<T>::step_and_validate(
					&mut bw_state,
					Either::Left(bhw_output),
					&bw_settings,
				)
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

				BWStatemachine::<T>::step_and_validate(&mut bw_state, input, &bw_settings).unwrap();

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
			write!(printed, "{:?}: {}, ", height.into_range_inclusive(), event).unwrap();
		}
		writeln!(printed).unwrap();
	}

	let counted_events: BTreeMultiSet<(
		ChainBlockNumberOf<<T as BWProcessorTypes>::Chain>,
		MockBtcEvent<Event>,
	)> = total_outputs.into_iter().flatten().collect();

	// verify that each event was emitted only one time
	for (event, count) in counted_events.0.clone() {
		if count > 1 {
			panic!(
				"Got event {event:?} in total {count} times           events: {printed}              bw_input_history: {}",
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
	assert_eq!(
		emitted_witness_events,
		expected_witness_events,
		"got witness events: {emitted_witness_events:?}, expected_witness_events: {expected_witness_events:?}, bw_input_history: {}",
		history.iter().map(|event| format!("{event:?}")).intersperse("\n".to_string()).collect::<String>()
	);
}

// ============================================================================
// Tests with regular block numbers (WITNESS_RANGE = 1, like Ethereum)
// ============================================================================

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
			run_simulation_generic::<Types>(blocks);
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

	run_simulation_generic::<Types>(ForkedFilledChain { blocks });
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

	run_simulation_generic::<Types>(ForkedFilledChain { blocks });
}

// ============================================================================
// Tests with BlockWitnessRange (WITNESS_RANGE = 24, like Arbitrum)
// ============================================================================

/// Generates random chain progressions and simulates running the witnessing electoral systems with
/// BlockWitnessRange as the ChainBlockNumber (like Arbitrum with WITNESS_PERIOD = 24).
#[test]
pub fn test_all_witness_ranges() {
	let mut runner = TestRunner::new(Config {
		source_file: Some(file!()),
		// Same number of cases as the regular test
		cases: 256 * 60,
		failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
			"proptest-regressions-full-pipeline-ranged",
		))),
		..Default::default()
	});
	runner
		.run(&generate_blocks_with_tail_ranged(), |blocks| {
			run_simulation_generic::<RangedTypes>(blocks);
			Ok(())
		})
		.unwrap();
}
