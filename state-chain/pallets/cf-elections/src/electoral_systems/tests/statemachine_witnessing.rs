//! This file tests the BlockWitnesser and the BlockHeightWitnesser state machines composed together
//! on realistic inputs of a chain with many reorgs.

use core::marker::PhantomData;
use std::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	hash::DefaultHasher,
};

use proptest::test_runner::{Config, TestRunner};

use crate::electoral_systems::{
	block_height_tracking::HeightWitnesserProperties,
	block_witnesser::{
		block_processor::{tests::MockBtcEvent, BlockProcessingInfo, BlockProcessorEvent},
		state_machine::{BWProcessorTypes, BWTypes},
	},
};
// use crate::electoral_systems::block_witnesser::block_processor::tests::MockBlockProcessorDefinition;
use crate::electoral_systems::{
	block_height_tracking::state_machine::{
		tests::*, BHWState, BHWStateWrapper, BlockHeightTrackingSM, InputHeaders,
	},
	block_witnesser::{
		block_processor::BlockProcessor,
		primitives::SafeModeStatus,
		state_machine::{
			tests::*, BWElectionProperties, BWStatemachine, BlockWitnesserSettings,
			BlockWitnesserState,
		},
	},
	state_machine::{
		chain::*,
		core::{hook_test_utils::MockHook, IndexedValidate, TypesFor},
		state_machine::Statemachine,
		state_machine_es::SMInput,
		test_utils::{BTreeMultiSet, Container},
	},
};

pub trait AbstractVoter<M: Statemachine> {
	fn vote(&mut self, index: M::InputIndex) -> Option<Vec<M::Input>>;
}

#[test]
pub fn test_all() {
	type N = u8;
	type BW = BWStatemachine<N>;
	type BHW = BlockHeightTrackingSM<N>;

	impl AbstractVoter<BHW> for FlatChainProgression<char> {
		fn vote(
			&mut self,
			indices: <BHW as Statemachine>::InputIndex,
		) -> Option<Vec<<BHW as Statemachine>::Input>> {
			let chain = self.get_next_chain()?;

			let mut result = Vec::new();

			for index in indices {
				let best_block = get_best_block(&chain);
				if best_block.block_height < index.witness_from_index {
					continue;
				}

				let bhw_input = match index {
					HeightWitnesserProperties { witness_from_index } =>
						if witness_from_index == 0 {
							println!("########### first witnessing #################");
							InputHeaders(VecDeque::from([best_block]))
						} else {
							let headers = (witness_from_index..=get_block_height(&chain))
								.map(|height| get_block_header(&chain, height));
							if headers.len() == 0 {
								println!("no new blocks, continuing...");
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
				let chain = self.get_next_chain()?;
				if let Some(block_data) = get_block_header(&chain, index.block_height) {
					inputs.push(SMInput::Consensus((
						index,
						block_data.hash.into_iter().map(|x| x as u8).collect(),
					)));
				}
			}
			Some(inputs)
		}
	}

	let mut runner = TestRunner::new(Config {
        cases: 256 * 16,
        ..Default::default()
    });
	runner.run(&generate_chain_progression(), |mut chains| {

        let mut bhw_state: BHWStateWrapper<N> = BHWStateWrapper {
            state: BHWState::Starting,
            block_height_update: MockHook::new(())
        };
        let block_processor: BlockProcessor<N> = BlockProcessor {
            blocks_data: Default::default(),
            processed_events: Default::default(),
            rules: Default::default(),
            execute: MockHook::new(()),
            delete_data: MockHook::new(()),
        };
        let mut bw_state: BlockWitnesserState<N> = BlockWitnesserState {
            elections: Default::default(),
            generate_election_properties_hook: Default::default(),
            safemode_enabled: MockHook::new(SafeModeStatus::Disabled),
            block_processor,
            _phantom: core::marker::PhantomData,
        };
        let bw_settings = BlockWitnesserSettings {
            max_concurrent_elections: 4,
            safety_margin: 7,
        };

        #[derive(Clone, Debug)]
        enum BWTrace<T: BWTypes> {
            Input(<BWStatemachine<T> as Statemachine>::Input),
            Output(Vec<(T::ChainBlockNumber, T::Event)>),
            Event(BlockProcessorEvent<T>)
        }

        let mut bw_history = Vec::new();

        let mut total_outputs = Vec::new();

        while chains.has_chains() {
            // run BHW
            let bhw_outputs = if let Some(inputs) = AbstractVoter::<BHW>::vote(&mut chains, BHW::input_index(&mut bhw_state)) {
                let mut outputs = Vec::new();
                for input in inputs {
                    let output = BHW::step(&mut bhw_state, input, &()).unwrap();
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

                    BW::step(&mut bw_state, input, &bw_settings).unwrap();

                    bw_history.extend(
                        bw_state.block_processor.delete_data
                        .take_history()
                        .into_iter()
                        .map(BWTrace::Event)
                    );

                    let mut output = bw_state.block_processor.execute.take_history();
                    println!("got output: {output:?}");
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

        println!("----- outputs begin ------");
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
        println!("{printed}");
        println!("----- outputs end ------");

        let counted_events : Container<BTreeMultiSet<(u8, MockBtcEvent)>> = total_outputs.into_iter().flatten().collect();

        for (event, count) in counted_events.0.0.clone() {
            if count > 1 {
                panic!("Got event {event:?} in total {count} times           events: {printed}              bw_input_history: {bw_history:?}");
            }
        }

        Ok(())
    }).unwrap();
}
