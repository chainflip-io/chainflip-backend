
use core::iter::Step;
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};

use itertools::Either;

use crate::electoral_systems::block_height_tracking::state_machine::MultiIndexAndValue;
use crate::electoral_systems::block_height_tracking::{
	state_machine::{ConstantIndex, IndexOf, StateMachine, Validate}, state_machine_es::SMInput, ChainProgress
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
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize,
)]
pub struct BWState<N: Ord, ElectionProperties, ElectionPropertiesHook: Hook<N,ElectionProperties>> {
	elections: ElectionTracker<N>,
    generate_election_properties_hook: ElectionPropertiesHook,
    _phantom: sp_std::marker::PhantomData<ElectionProperties>
}

impl<N: Ord + Step, ElectionProperties, ElectionPropertiesHook: Hook<N,ElectionProperties>> Validate for BWState<N, ElectionProperties, ElectionPropertiesHook> {
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

	type Input = SMInput<MultiIndexAndValue<(N, ElectionProperties, u8), BlockData>, ChainProgress<N>>;
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
				s.elections.schedule_range(range);
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
		use itertools::Itertools;
		use SMInput::*;
		use ChainProgress::*;

		assert!(
			// there should always be at most as many elections as given in the settings
			// or more if we had more elections previously
			after.elections.ongoing.len() <= sp_std::cmp::max(settings.max_concurrent_elections as usize, before.elections.ongoing.len()), 
			"too many concurrent elections"
		);

		// all new elections use the current reorg id as index
		assert!(
			after.elections.ongoing.iter().all(
				|(height, ix)| if before.elections.ongoing.get(&height).is_none() {
					*ix == after.elections.reorg_id
				} else {
					true
				}
			),
			"new election with wrong index"
		);

		match input {
			Vote(MultiIndexAndValue((height, _, _), _)) => {

				// the elections after a vote are the ones from before, minus the voted one + all outstanding ones
				let after_should = before.elections.ongoing.key_set().without(*height)
							.merge((before.elections.next_election ..= before.elections.highest_scheduled).take((settings.max_concurrent_elections as usize + 1).saturating_sub(before.elections.ongoing.len())).collect());

				assert_eq!(
					after.elections.ongoing.key_set(), after_should,
					"wrong ongoing election set after received vote",
				)
			},

			Context(Reorg(range) | Continuous(range)) => {
				// if there is a reorg, the new reorg index must be different than the indices of the previously ongoing elections
				assert!(
					if *range.start() < before.elections.next_election {
						before.elections.ongoing.iter().all(|(_, ix)| *ix != after.elections.reorg_id)
					} else {
						true
					},
					"wrong reorg_counter after reorg"
				);

				// if an ongoing election is not part of the reorg range, it should not be stopped or restarted
				assert!(before.elections.ongoing.iter().all(
					|(height, ix)| if height < range.start() {after.elections.ongoing.iter().contains(&(height, ix))} else {true}
				), "ongoing election which wasn't part of reorg should stay open");

				// if an ongoing election is part of the reorg range, it should should either be removed or restarted with the new index
				assert!(before.elections.ongoing.iter().all(
					|(height, ix)| if height >= range.start() {
						after.elections.ongoing.get(&height).is_none_or(|index| *index == after.elections.reorg_id)
					} else {
						true
					}
				), "ongoing election which was part of reorg should stay open with new index (after.ongoing = {:?})", after.elections.ongoing);
			},

			Context(WaitingForFirstConsensus | None(_)) => (),
		}
	}

}

#[cfg(test)]
mod tests {
	use core::iter::Filter;

	use proptest::{
		prelude::{any, prop, Arbitrary, BoxedStrategy, Just, Strategy},
		prop_oneof,
	};

    use crate::prop_do;

    use super::*;
    use hook_test_utils::*;

    type SM = BWStateMachine<(), (), u8, ConstantHook<u8, ()>>;

	// we generate (a,b) with a <= b
	fn ordered_pair<N: Step + Arbitrary>() -> impl Strategy<Value = (N, N)> {
		prop_do!{
			let (x,y) in any::<(N, N)>();
			let (a,b) = if N::steps_between(&x,&y).1 == None {
				(y, x)
			} else {
				(x,y)
			};
			return (a,b)
		}
	}

    fn generate_state<N: BlockZero + Arbitrary + Step + Ord + Copy>() -> impl Strategy<Value = BWState<N, (), ConstantHook<N, ()>>> {

		prop_do!{
			let next_election in any::<N>();
			let highest_scheduled in any::<N>();
			let reorg_id in any::<u8>();
			let ongoing in prop::collection::vec((any::<N>(), any::<u8>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move |(height, _)| *height < next_election));
			return BWState {
                elections: ElectionTracker {
                    next_election: next_election.clone(),
                    highest_scheduled: highest_scheduled.clone(),
                    ongoing: BTreeMap::from_iter(ongoing),
					reorg_id
                },
                generate_election_properties_hook: ConstantHook { state: (), _phantom: Default::default() },
                _phantom: core::marker::PhantomData,
            }
		}
    }

    fn generate_input(index: IndexOf<<SM as StateMachine>::Input>) -> BoxedStrategy<<SM as StateMachine>::Input> {

		let generate_input = |index| {

			prop_oneof![
				Just(SMInput::Vote(MultiIndexAndValue(index, ()))),
				prop_oneof![
					Just(ChainProgress::WaitingForFirstConsensus),
					any::<u8>().prop_map(ChainProgress::None),
					any::<(u8, u8)>().prop_map(|(a,b)| ChainProgress::Continuous(a..=a)),
					any::<(u8, u8)>().prop_map(|(a,b)| ChainProgress::Reorg(a..=a))
				].prop_map(SMInput::Context)
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
			file!(),
            generate_state(),
			prop_do!{
				// let safe_mode_enabled in any::<bool>();
				// let max_concurrent_elections in 1..10u32;
				return BWSettings { safe_mode_enabled: false, max_concurrent_elections: 5 }
			},
			generate_input
        );
    }
}

