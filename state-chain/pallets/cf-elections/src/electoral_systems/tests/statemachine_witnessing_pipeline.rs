//! This file tests the BlockWitnesser and the BlockHeightWitnesser state machines composed together
//! on realistic inputs of a chain with many reorgs.

pub mod chainstate_simulation;

use core::marker::PhantomData;
use std::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	hash::DefaultHasher,
};

use frame_support::ensure;
use itertools::Itertools;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};

use crate::electoral_systems::{
	block_height_tracking::{
		state_machine::{tests::*, BHWState, BHWStateWrapper, BlockHeightTrackingSM, InputHeaders},
		HWTypes, HeightWitnesserProperties,
	},
	block_witnesser::{
		block_processor::{
			tests::MockBtcEvent, BlockProcessingInfo, BlockProcessor, BlockProcessorEvent,
		},
		primitives::SafeModeStatus,
		state_machine::{
			tests::*, BWElectionProperties, BWElectionType, BWProcessorTypes, BWStatemachine,
			BWTypes, BlockWitnesserSettings, BlockWitnesserState,
		},
	},
	state_machine::{
		core::{hook_test_utils::MockHook, IndexedValidate, TypesFor, Validate},
		state_machine::Statemachine,
		state_machine_es::SMInput,
		test_utils::{BTreeMultiSet, Container},
	},
};
use chainstate_simulation::*;

macro_rules! if_matches {
    ($($tt:tt)+) => {
        |x| matches!(x, $($tt)+)
    };
}

macro_rules! try_get {
    ($($tt:tt)+) => {
        |x| match x {$($tt)+(x) => Some(x), _ => None}
    };
}

pub trait AbstractVoter<M: Statemachine> {
	fn vote(&mut self, index: M::InputIndex) -> Option<Vec<M::Input>>;
}

#[test]
pub fn test_all() {
	type Types = (u8, usize, Vec<char>);
	type BW = BWStatemachine<Types>;
	type BHW = BlockHeightTrackingSM<Types>;

	const OFFSET: usize = 20;

	impl AbstractVoter<BHW> for FlatChainProgression<char> {
		fn vote(
			&mut self,
			indices: <BHW as Statemachine>::InputIndex,
		) -> Option<Vec<<BHW as Statemachine>::Input>> {
			let chain = MockChain::new_with_offset(OFFSET, self.get_next_chain()?);

			let mut result = Vec::new();

			for index in indices {
				let best_block = chain.get_best_block_header();
				if best_block.block_height < index.witness_from_index {
					continue;
				}

				let bhw_input = match index {
					HeightWitnesserProperties { witness_from_index } =>
						if witness_from_index == 0 {
							InputHeaders(VecDeque::from([best_block]))
						} else {
							let headers = (witness_from_index..=chain.get_best_block_height())
								.map(|height| chain.get_block_header(height));
							if headers.len() == 0 {
								continue;
							}
							if let Some(headers) = headers.into_iter().collect::<Option<Vec<_>>>() {
								InputHeaders(VecDeque::from_iter(headers))
							} else {
								continue
							}
						},
				};

				result.push(SMInput::Consensus((index, bhw_input)));
			}

			Some(result)
		}
	}

	impl AbstractVoter<BW> for FlatChainProgression<char> {
		fn vote(
			&mut self,
			indices: <BW as Statemachine>::InputIndex,
		) -> Option<Vec<<BW as Statemachine>::Input>> {
			let mut inputs = Vec::new();
			for index in indices {
				let chain =
					MockChain::<char, Types>::new_with_offset(OFFSET, self.get_next_chain()?);

				use BWElectionType::*;
				match index.election_type {
					Optimistic =>
						if let Some(block_data) = chain.get_block_by_height(index.block_height) {
							let header = chain.get_block_header(index.block_height).unwrap();
							inputs
								.push(SMInput::Consensus((index, (block_data, Some(header.hash)))));
						},
					ByHash(hash) =>
						if let Some(block_data) = chain.get_block_by_hash(hash) {
							inputs.push(SMInput::Consensus((index, (block_data, None))));
						},
					SafeBlockHeight =>
						if let Some(block_data) = chain.get_block_by_height(index.block_height) {
							inputs.push(SMInput::Consensus((index, (block_data, None))));
						},
				}
			}
			Some(inputs)
		}
	}

	let mut runner = TestRunner::new(Config {
		cases: 256 * 16 * 16,
		failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
			"proptest-regressions-full-pipeline",
		))),
		..Default::default()
	});
	runner.run(&generate_blocks_with_tail(), |blocks| {

        let mut chains = blocks_into_chain_progression(&blocks.blocks);

        const SAFETY_MARGIN: u32 = 7;

        // get final chain so we can check that we emitted the correct events:
        let final_chain = chains.get_final_chain();
        let finalized_blocks : Vec<_> = final_chain;
        let finalized_events : BTreeSet<_> = finalized_blocks.iter().flat_map(|block| block.events.iter()).collect();

        // prepare the state machines
        let mut bhw_state: BHWStateWrapper<Types> = BHWStateWrapper {
            state: BHWState::Starting,
            block_height_update: MockHook::new(())
        };
        let block_processor: BlockProcessor<Types> = BlockProcessor {
            blocks_data: Default::default(),
            processed_events: Default::default(),
            rules: Default::default(),
            execute: MockHook::new(()),
            delete_data: MockHook::new(()),
        };
        let mut bw_state: BlockWitnesserState<Types> = BlockWitnesserState {
            elections: Default::default(),
            generate_election_properties_hook: Default::default(),
            safemode_enabled: MockHook::new(SafeModeStatus::Disabled),
            block_processor,
            _phantom: core::marker::PhantomData,
            optimistic_blocks_cache: Default::default(),
        };
        let bw_settings = BlockWitnesserSettings {
            max_concurrent_elections: 4,
            safety_margin: SAFETY_MARGIN,
        };

        #[derive(Clone, Debug)]
        enum BWTrace<T: BWTypes, T0: HWTypes> {
            Input(<BWStatemachine<T> as Statemachine>::Input),
            InputBHW(<BlockHeightTrackingSM<T0> as Statemachine>::Input),
            Output(Vec<(T::ChainBlockNumber, T::Event)>),
            Event(BlockProcessorEvent<T>)
        }

        let mut bw_history = Vec::new();
        let mut total_outputs = Vec::new();

        let print_bw_history = |bw_history: &Vec<BWTrace<Types, Types>>| 
            bw_history.iter().map(|event| format!("{event:?}")).intersperse("\n".to_string()).collect::<String>();

        while chains.has_chains() {
            // run BHW
            let bhw_outputs = if let Some(inputs) = AbstractVoter::<BHW>::vote(&mut chains, BHW::input_index(&mut bhw_state)) {
                let mut outputs = Vec::new();
                for input in inputs {
                    // ensure that input is correct
                    BHW::validate(&BHW::input_index(&mut bhw_state), &input).unwrap();

                    bw_history.push(BWTrace::InputBHW(input.clone()));

                    let output = BHW::step(&mut bhw_state, input, &())
                    .map_err(|err| format!("err: {err} with history: {bw_history:?}"))
                    .unwrap();
                    // .expect(&format!("BHW failed with history: {bw_history:?}"));

                    outputs.push(output);
                }
                outputs
            } else {
                break
            };

            // ---- BW ----

            let mut bw_outputs = if let Some(inputs) = AbstractVoter::<BW>::vote(&mut chains, BW::input_index(&mut bw_state)) {
                let mut outputs = Vec::new();

                // run BW on BHW outputs (context)
                for bhw_output in bhw_outputs {
                    bw_history.push(BWTrace::Input(SMInput::Context(bhw_output.clone())));

                    bw_state.elections.is_valid()
                        .map_err(|err| format!("err: {err} with history: {}", print_bw_history(&bw_history)))
                        .unwrap();

                    BW::step(&mut bw_state, SMInput::Context(bhw_output), &bw_settings).unwrap();

                    bw_history.extend(
                        bw_state.block_processor.delete_data
                        .take_history()
                        .into_iter()
                        .map(BWTrace::Event));

                    let mut output = bw_state.block_processor.execute.take_history();
                    bw_history.extend(
                        output.iter().cloned().map(BWTrace::Output)
                    );

                    outputs.append(&mut output);
                }

                // run on BW inputs (consensus)
                for input in inputs {
                    bw_history.push(BWTrace::Input(input.clone()));

                    bw_state.elections.is_valid()
                        .map_err(|err| format!("err: {err} with history: {}", print_bw_history(&bw_history)))
                        .unwrap();

                    BW::step(&mut bw_state, input, &bw_settings).unwrap();

                    bw_history.extend(
                        bw_state.block_processor.delete_data
                        .take_history()
                        .into_iter()
                        .map(BWTrace::Event)
                    );

                    let mut output = bw_state.block_processor.execute.take_history();
                    bw_history.extend(
                        output.iter().cloned().map(BWTrace::Output)
                    );

                    outputs.append(&mut output);
                }
                outputs
            } else {
                break
            };

            total_outputs.append(&mut bw_outputs);
        }

        // println!("----- outputs begin ------");
        use std::fmt::Write;
        let mut printed: String = Default::default();
        for output in total_outputs.clone() {
            if output.len() == 0 {
                write!(printed, "No events").unwrap();
            }
            for (height, event) in output {
                let event = match event {
                    MockBtcEvent::PreWitness(data) => format!("Pre {}", data as char),
                    MockBtcEvent::Witness(data) => format!("Wit {}", data as char),
                };
                write!(printed, "{height}: {}, ", event).unwrap();
            }
            writeln!(printed, "").unwrap();
        }

        let counted_events : Container<BTreeMultiSet<(u8, MockBtcEvent<char>)>> = total_outputs.into_iter().flatten().collect();

        // verify that each event was emitted only one time 
        for (event, count) in counted_events.0.0.clone() {
            if count > 1 {
                panic!("Got event {event:?} in total {count} times           events: {printed}              bw_input_history: {bw_history:?}");
            }
        }

        // ensure that we only emit witness events that are on the final chain
        let emitted_witness_events : BTreeSet<_> = counted_events.0.0.keys().map(|(a,b)|b).filter_map(try_get!(MockBtcEvent::Witness)).map(|event| *event as char).collect();
        let expected_witness_events : BTreeSet<_> = finalized_events.into_iter().cloned().collect();
        assert!(emitted_witness_events == expected_witness_events,
            "got witness events: {emitted_witness_events:?}, expected_witness_events: {expected_witness_events:?}, bw_input_history: {}",
            bw_history.iter().map(|event| format!("{event:?}")).intersperse("\n".to_string()).collect::<String>()
        );

        Ok(())
    }).unwrap();
}
