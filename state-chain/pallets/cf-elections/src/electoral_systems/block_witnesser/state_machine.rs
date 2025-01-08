
use core::iter::Step;
use cf_chains::witness_period::BlockZero;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};


use crate::electoral_systems::state_machine::core::{MultiIndexAndValue, IndexOf, Validate};
use crate::electoral_systems::state_machine::state_machine::StateMachine;
use crate::electoral_systems::state_machine::state_machine_es::SMInput;
use crate::electoral_systems::block_height_tracking::ChainProgress;
use super::primitives::ElectionTracker;
use super::super::state_machine::core::*;


pub trait BWTypes<N> : 'static {
	type ElectionProperties : PartialEq + Clone + sp_std::fmt::Debug + 'static;
	type ElectionPropertiesHook: Hook<N, Self::ElectionProperties>;
	type SafeModeEnabledHook: Hook<(), bool>;
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BWSettings {
	pub max_concurrent_elections: u32,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize,
)]
pub struct BWState<N: Ord, Types: BWTypes<N>> {
	elections: ElectionTracker<N>,
    generate_election_properties_hook: Types::ElectionPropertiesHook,
	safemode_enabled: Types::SafeModeEnabledHook,
    _phantom: sp_std::marker::PhantomData<Types>
}

impl<N: Ord + Step, Types: BWTypes<N>> Validate for BWState<N, Types> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<N: BlockZero + Ord, Types: BWTypes<N>> Default for BWState<N, Types> 
    where
		Types::ElectionPropertiesHook: Default,
		Types::SafeModeEnabledHook: Default,
{
	fn default() -> Self {
		Self { elections: Default::default(), generate_election_properties_hook: Default::default(), safemode_enabled: Default::default(), _phantom: Default::default() }
	}
}


pub struct BWStateMachine<
	Types: BWTypes<N>,
	BlockData,
	N,
	> {
	_phantom: sp_std::marker::PhantomData<(Types, BlockData, N)>,
}

impl<
	N : Copy + Ord + Step + sp_std::fmt::Debug + 'static,
	Types: BWTypes<N>,
	BlockData: PartialEq + Clone + sp_std::fmt::Debug + 'static,
> StateMachine for BWStateMachine<Types, BlockData, N> {

	type Input = SMInput<MultiIndexAndValue<(N, Types::ElectionProperties, u8), BlockData>, ChainProgress<N>>;
	type Settings = BWSettings;
	type Output = Result<(), &'static str>;
	type State = BWState<N, Types>;

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

		if !s.safemode_enabled.run(()) {
			s.elections.start_more_elections(settings.max_concurrent_elections as usize);
		}

		log::info!("BW: done. current elections: {:?}", s.elections.ongoing);

		Ok(())
	}

    /// Specifiation for step function
	#[cfg(test)]
	fn step_specification(before: &Self::State, input: &Self::Input, settings: &Self::Settings, after: &Self::State) {
		use std::collections::BTreeSet;

		use itertools::Itertools;
		use SMInput::*;
		use ChainProgress::*;

		use crate::electoral_systems::block_witnesser::helpers::*;

		let safemode_enabled = before.safemode_enabled.run(());

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

				let new_elections = if before.safemode_enabled.run(()) {
					BTreeSet::new()
				} else {
					(before.elections.next_election ..= before.elections.highest_scheduled).take((settings.max_concurrent_elections as usize + 1).saturating_sub(before.elections.ongoing.len())).collect()
				};

				// the elections after a vote are the ones from before, minus the voted one + all outstanding ones
				let after_should = before.elections.ongoing.key_set().without(*height).merge(new_elections);

				assert_eq!(
					after.elections.ongoing.key_set(), after_should,
					"wrong ongoing election set after received vote",
				)
			},

			Context(Reorg(range) | Continuous(range)) => {

				if !safemode_enabled {

					if *range.start() < before.elections.next_election {
						assert!(
							before.elections.ongoing.iter().all(|(_, ix)| *ix != after.elections.reorg_id),
							"if there is a reorg, the new reorg_id must be different than the ids of the previously ongoing elections"
						)
					} else {
						assert_eq!(
							before.elections.reorg_id, after.elections.reorg_id,
							"if there is no reorg, the reorg_id should stay the same"
						);
					}

					for (height, ix) in &before.elections.ongoing {
						if height < range.start() {
							assert!(
								after.elections.ongoing.iter().contains(&(height, ix)), 
								"ongoing election which wasn't part of reorg should stay open with same index. (after.ongoing = {:?})", after.elections.ongoing
							);
						} else {
							assert!(
								after.elections.ongoing.get(&height).is_none_or(|index| *index == after.elections.reorg_id),
								"ongoing election which was part of reorg should either be removed or stay open with new index (after.ongoing = {:?})", after.elections.ongoing
							)
						}
					}

				} else {

					// if safe mode is enabled, ongoing elections shouldn't change
					assert!(
						before.elections.ongoing == after.elections.ongoing,
						"if safemode is enabled, ongoing elections shouldn't change (except being closed when they come to consensus)"
					);

					// but the next_election should still be updated in order to restart the reorg'ed elections after safemode is disabled
					if *range.start() < before.elections.next_election {
						assert!(
							after.elections.next_election == *range.start(),
							"if safemode is enabled, and a reorg happens, next_election should be the start of the reorg range"
						);
					} else {
						assert!(
							after.elections.next_election == before.elections.next_election,
							"if safemode is enabled, and no (relevant) reorg happens, next_election should not be udpated"
						);
					}
				}
			},

			Context(WaitingForFirstConsensus | None(_)) => (),
		}
	}

}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use proptest::{
		prelude::{any, prop, Arbitrary, BoxedStrategy, Just, Strategy},
		prop_oneof,
	};

    use crate::prop_do;

    use super::*;
    use hook_test_utils::*;

	impl<N: Ord> BWTypes<N> for () {
		type ElectionProperties = ();
		type ElectionPropertiesHook = ConstantHook<N, ()>;
		type SafeModeEnabledHook = ConstantHook<(), bool>;
	}
	
    type SM = BWStateMachine<(), (), u8>;

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

    fn generate_state<N: BlockZero + Arbitrary + Step + Ord + Copy>() -> impl Strategy<Value = BWState<N, ()>> {

		prop_do!{
			let next_election in any::<N>();
			let highest_scheduled in any::<N>();
			let reorg_id in any::<u8>();
			let safemode_enabled in any::<bool>();
			let ongoing in prop::collection::vec((any::<N>(), any::<u8>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move |(height, _)| *height < next_election));
			return BWState {
                elections: ElectionTracker {
                    next_election: next_election.clone(),
                    highest_scheduled: highest_scheduled.clone(),
                    ongoing: BTreeMap::from_iter(ongoing),
					reorg_id
                },
                generate_election_properties_hook: ConstantHook::new(()),
				safemode_enabled: ConstantHook::new(safemode_enabled),
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
					any::<(u8, u8)>().prop_map(|(a,b)| ChainProgress::Continuous(a..=a.saturating_add(b))),
					any::<(u8, u8)>().prop_map(|(a,b)| ChainProgress::Reorg(a..=a.saturating_add(b)))
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
				let max_concurrent_elections in 0..10u32;
				return BWSettings { max_concurrent_elections }
			},
			generate_input
        );
    }
}

