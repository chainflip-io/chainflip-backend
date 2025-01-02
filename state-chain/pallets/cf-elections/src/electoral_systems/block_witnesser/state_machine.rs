
use core::{iter::Step, ops::RangeInclusive};
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use frame_support::{ensure, Hashable};
use log::trace;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};
use sp_std::vec::Vec;
use sp_std::ops::Add;

use itertools::Either;

use crate::electoral_systems::block_height_tracking::state_machine::IndexAndValue;
use crate::electoral_systems::block_height_tracking::{
	consensus::{ConsensusMechanism, SupermajorityConsensus, Threshold}, state_machine::{ConstantIndex, IndexOf, StateMachine, Validate}, state_machine_es::SMInput, ChainProgress
};
use crate::{SharedData, SharedDataHash};
use super::primitives::ElectionTracker;
use super::super::state_machine::core::*;
use super::helpers::*;


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWSettings {
	pub safe_mode_enabled: bool,
	pub max_concurrent_elections: u32,
}


#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWState<N: Ord, ElectionProperties, ElectionPropertiesHook: Hook<N,ElectionProperties>> {
	elections: ElectionTracker<N>,
    generate_election_properties_hook: ElectionPropertiesHook,
    _phantom: sp_std::marker::PhantomData<ElectionProperties>
}

impl<N: Ord, ElectionProperties, ElectionPropertiesHook: Hook<N,ElectionProperties>> Validate for BWState<N, ElectionProperties, ElectionPropertiesHook> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<N: BlockZero + Ord, ElectionProperties, ElectionPropertiesHook: Hook<N,ElectionProperties>> Default for BWState<N, ElectionProperties, ElectionPropertiesHook> 
    where ElectionPropertiesHook: Default
{
	fn default() -> Self {
		Self { elections: Default::default(), generate_election_properties_hook: Default::default(), _phantom: Default::default() }
	}
}



pub struct BWStateMachine<
	ElectionProperties,
	BlockData,
	N,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties>
	> {
	_phantom: sp_std::marker::PhantomData<(ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator)>,
}

impl<
	ElectionProperties: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	BlockData: PartialEq + Clone + sp_std::fmt::Debug + 'static,
	N : Copy + Ord + Step + sp_std::fmt::Debug + 'static,
	BlockElectionPropertiesGenerator: Hook<N, ElectionProperties> + 'static
> StateMachine for BWStateMachine<ElectionProperties, BlockData, N, BlockElectionPropertiesGenerator> {

	type Input = SMInput<IndexAndValue<(N, ElectionProperties, u32), BlockData>, ChainProgress<N>>;
	type Settings = BWSettings;
	type Output = Result<(), &'static str>;
	type State = BWState<N, ElectionProperties, BlockElectionPropertiesGenerator>;

	fn input_index(s: &Self::State) -> Vec<IndexOf<Self::Input>> {
		s.elections.ongoing.clone().into_iter().map(|(height, extra)| (height, s.generate_election_properties_hook.run(height), extra)).collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, settings: &Self::Settings) -> Self::Output {
		log::info!("BW: input {i:?}");
		match i {
			SMInput::Context(ChainProgress::Reorg(range) | ChainProgress::Continuous(range)) => {
				s.elections.schedule_up_to(*range.end());
				for election in range {
					s.elections.restart_election(election);
				}
			},

			SMInput::Context(ChainProgress::WaitingForFirstConsensus | ChainProgress::None(_)) => {},

			SMInput::Vote(blockdata) => {
				// insert blockdata into our cache of blocks
				s.elections.mark_election_done(blockdata.0.0);
				log::info!("got block data: {:?}", blockdata.1);
			},
		};

		if !settings.safe_mode_enabled {
			s.elections.start_more_elections(settings.max_concurrent_elections as usize);
		}

		log::info!("BW: done. current elections: {:?}", s.elections.ongoing);

		Ok(())
	}

    /// Specifiation for step function
	#[cfg(test)]
	fn step_specification(before: &Self::State, input: &Self::Input, settings: &Self::Settings, after: &Self::State) {
		use SMInput::*;
		use ChainProgress::*;

		assert!(
			// there should always be at most as many elections as given in the settings
			after.elections.ongoing.len() <= settings.max_concurrent_elections as usize, 
			"too many concurrent elections"
		);

		match input {
			Vote(IndexAndValue((height, _, _), _)) => {
				assert!(
					// after receiving a vote, the ongoing elections should be the same as previously,
					// except with the vote's height removed
					after.elections.ongoing.key_set().with(*height) == before.elections.ongoing.key_set(),
					"wrong ongoing election set after received vote"
				)
			},

			Context(Reorg(range) | Continuous(range)) => {

				let all_elections = before.elections.ongoing.key_set().merge(range.clone().into_set());

				assert!(
					// There should be exactly those elections ongoing which have the lowest heights (at most max_concurrent_elections of them)
					all_elections.into_iter().take(settings.max_concurrent_elections as usize).collect::<BTreeSet<_>>() == after.elections.ongoing.key_set(),
					"wrong ongoing election after receiving new block height range"
				)
			},

			Context(WaitingForFirstConsensus | None(_)) => (),
		}
	}

}

#[cfg(test)]
mod tests {
	use proptest::{
		prelude::{any, prop, Arbitrary, Just, Strategy},
		prop_oneof, proptest,
	};

    use super::*;
    use super::super::super::state_machine::core::*;
    use hook_test_utils::*;

    type SM = BWStateMachine<(), (), u8, ConstantHook<u8, ()>>;

    fn generate_state<N: Arbitrary + Step + Ord + Clone>() -> impl Strategy<Value = BWState<N, (), ConstantHook<N, ()>>> {

        (any::<N>(), any::<usize>(), prop::collection::vec(any::<(N, u32)>(), 0..10)).prop_map(
            |(highest_started, scheduled_not_started, ongoing)| BWState {
                elections: ElectionTracker {
                    highest_started: highest_started.clone(),
                    highest_scheduled: N::forward(highest_started, scheduled_not_started),
                    ongoing: BTreeMap::from_iter(ongoing.into_iter()) 
                },
                generate_election_properties_hook: ConstantHook { state: (), _phantom: Default::default() },
                _phantom: core::marker::PhantomData,
            }
        )
    }

    fn generate_input(index: IndexOf<<SM as StateMachine>::Input>) -> impl Strategy<Value = <SM as StateMachine>::Input> {
        let context = prop_oneof![
            Just(ChainProgress::WaitingForFirstConsensus)
        ];

        prop_oneof![
            Just(SMInput::Vote(IndexAndValue(index, ()))),
            context.prop_map(SMInput::Context)
        ]
    }

    #[test]
    pub fn test_bw_statemachine() {
        BWStateMachine::<(), (), u8, ConstantHook<u8, ()>>::test(
            generate_state(),
            Just(BWSettings { safe_mode_enabled: false, max_concurrent_elections: 5 }),
            |index| 
				(0..index.len()).prop_flat_map(move |ix| generate_input(
					index.clone().into_iter().nth(ix).unwrap()
				)).boxed()
        );
    }
}

