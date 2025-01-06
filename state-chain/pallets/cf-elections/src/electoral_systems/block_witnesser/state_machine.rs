
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

use crate::electoral_systems::block_height_tracking::state_machine::MultiIndexAndValue;
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

	type Input = SMInput<MultiIndexAndValue<(N, ElectionProperties, u32), BlockData>, ChainProgress<N>>;
	type Settings = BWSettings;
	type Output = Result<(), &'static str>;
	type State = BWState<N, ElectionProperties, BlockElectionPropertiesGenerator>;

	fn input_index(s: &Self::State) -> IndexOf<Self::Input> {
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
			// or more if we had more elections previously
			after.elections.ongoing.len() <= sp_std::cmp::max(settings.max_concurrent_elections as usize, before.elections.ongoing.len()), 
			"too many concurrent elections"
		);

		match input {
			Vote(MultiIndexAndValue((height, _, _), _)) => {
				// the elections after a vote are the ones from before, minus the voted one + all outstanding ones
				let after_should = before.elections.ongoing.key_set().without(*height)
							.merge((N::forward(before.elections.highest_started, 1) .. before.elections.highest_scheduled).take((settings.max_concurrent_elections as usize + 1).saturating_sub(before.elections.ongoing.len())).collect());

				assert_eq!(
					// after receiving a vote, the ongoing elections should be the same as previously,
					// except with the vote's height removed
					after.elections.ongoing.key_set(), after_should,
					"wrong ongoing election set after received vote",
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
		prelude::{any, prop, Arbitrary, BoxedStrategy, Just, Strategy},
		prop_oneof, proptest,
	};

    use crate::prop_do;

    use super::*;
    use super::super::super::state_machine::core::*;
    use hook_test_utils::*;

    type SM = BWStateMachine<(), (), u64, ConstantHook<u64, ()>>;


    fn generate_state<N: BlockZero + Arbitrary + Step + Ord + Clone>() -> impl Strategy<Value = BWState<N, (), ConstantHook<N, ()>>> {

		let into_n = |x: usize| N::saturating_forward(N::zero(), x);

		prop_do!{
			let highest_started_u = in any::<usize>();
			let scheduled_not_started = in any::<usize>();
			let highest_started = into_n(highest_started_u);
			let highest_scheduled = into_n(highest_started_u.saturating_add(scheduled_not_started));
			let ongoing = in prop::collection::vec(((0..highest_started_u).prop_map(into_n), any::<u32>()), 0..10);
			return BWState {
                elections: ElectionTracker {
                    highest_started: highest_started.clone(),
                    highest_scheduled: highest_scheduled.clone(),
                    ongoing: BTreeMap::from_iter(ongoing.into_iter()) 
                },
                generate_election_properties_hook: ConstantHook { state: (), _phantom: Default::default() },
                _phantom: core::marker::PhantomData,
            }
		}
    }

    fn generate_input(index: IndexOf<<SM as StateMachine>::Input>) -> BoxedStrategy<<SM as StateMachine>::Input> {

		let generate_input = |index| {
			let context = prop_oneof![
				Just(ChainProgress::WaitingForFirstConsensus)
			];

			prop_oneof![
				Just(SMInput::Vote(MultiIndexAndValue(index, ()))),
				context.prop_map(SMInput::Context)
			]
		};

		if index.len() > 0 {
			(0..index.len()).prop_flat_map(move |ix| generate_input(
				index.clone().into_iter().nth(ix).unwrap()
			)).boxed()
		} else {
			Just(SMInput::Context(ChainProgress::WaitingForFirstConsensus)).boxed()
		}

    }


    #[test]
    pub fn test_bw_statemachine() {
        SM::test(
            generate_state(),
            Just(BWSettings { safe_mode_enabled: false, max_concurrent_elections: 5 }),
			generate_input
        );
    }
}

