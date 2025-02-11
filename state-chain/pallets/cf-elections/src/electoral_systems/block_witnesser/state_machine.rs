use super::{
	super::state_machine::core::*,
	primitives::{ElectionTracker, SafeModeStatus},
};
use crate::electoral_systems::{
	block_height_tracking::ChainProgress,
	block_witnesser::{block_processor::BlockProcessor, primitives::ChainProgressInner},
	state_machine::{
		core::{IndexOf, MultiIndexAndValue, Validate},
		state_machine::StateMachine,
		state_machine_es::SMInput,
	},
};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use core::iter::Step;
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, ops::Sub, vec::Vec};

pub trait BWTypes: 'static {
	type ChainBlockNumber: Serde
		+ Copy
		+ Eq
		+ Ord
		+ SaturatingStep
		+ Step
		+ BlockZero
		+ Debug
		+ 'static;
	type BlockData: PartialEq + Clone + Debug + Eq + 'static;
	type ElectionProperties: PartialEq + Clone + Eq + Debug + 'static;
	type ElectionPropertiesHook: Hook<Self::ChainBlockNumber, Self::ElectionProperties>;
	type SafeModeEnabledHook: Hook<(), SafeModeStatus>;
	type BWProcessorTypes: BWProcessorTypes<
		ChainBlockNumber = Self::ChainBlockNumber,
		BlockData = Self::BlockData,
	>;
}

pub trait BWProcessorTypes {
	type ChainBlockNumber: Serde
		+ Copy
		+ Eq
		+ Ord
		+ SaturatingStep
		+ Step
		+ BlockZero
		+ Debug
		+ Into<u64>
		+ Default
		+ From<u64>
		+ Sub<Output = Self::ChainBlockNumber>
		+ 'static;

	type BlockData: Serde + Clone;
	type Event: Serde + Debug + Clone + Eq;
	type Rules: Hook<
			(Self::ChainBlockNumber, Self::ChainBlockNumber, Self::BlockData),
			Vec<(Self::ChainBlockNumber, Self::Event)>,
		> + Default
		+ Serde
		+ Debug
		+ Clone
		+ Eq;
	type Execute: Hook<(Self::ChainBlockNumber, Self::Event), ()>
		+ Default
		+ Serde
		+ Debug
		+ Clone
		+ Eq;
	type DedupEvents: Hook<Vec<(Self::ChainBlockNumber, Self::Event)>, Vec<(Self::ChainBlockNumber, Self::Event)>>
		+ Default
		+ Serde
		+ Debug
		+ Clone
		+ Eq;

	type SafetyMargin: Hook<(), Self::ChainBlockNumber> + Default + Serde + Debug + Clone + Eq;
}

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Deserialize,
	Serialize,
	Ord,
	PartialOrd,
	Default,
)]
pub struct BWSettings {
	pub max_concurrent_elections: u16,
}

#[derive_where(Debug, Clone, PartialEq, Eq; T::SafeModeEnabledHook: Debug + Clone + Eq, T::ElectionPropertiesHook: Debug + Clone + Eq, T::BWProcessorTypes: Debug + Clone + Eq)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct BWState<T: BWTypes> {
	pub elections: ElectionTracker<T::ChainBlockNumber>,
	pub generate_election_properties_hook: T::ElectionPropertiesHook,
	pub safemode_enabled: T::SafeModeEnabledHook,
	pub block_processor: BlockProcessor<T::BWProcessorTypes>,
	_phantom: sp_std::marker::PhantomData<T>,
}

impl<T: BWTypes> Validate for BWState<T> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<T: BWTypes> Default for BWState<T>
where
	T::ElectionPropertiesHook: Default,
	T::SafeModeEnabledHook: Default,
{
	fn default() -> Self {
		Self {
			elections: Default::default(),
			generate_election_properties_hook: Default::default(),
			safemode_enabled: Default::default(),
			block_processor: Default::default(),
			_phantom: Default::default(),
		}
	}
}

pub struct BWStateMachine<Types: BWTypes> {
	_phantom: sp_std::marker::PhantomData<Types>,
}

impl<T: BWTypes> StateMachine for BWStateMachine<T> {
	type Input = SMInput<
		MultiIndexAndValue<
			(T::ChainBlockNumber, T::ElectionProperties, u8),
			ConstantIndex<(T::ChainBlockNumber, T::ElectionProperties, u8), T::BlockData>,
		>,
		ChainProgress<T::ChainBlockNumber>,
	>;
	type Settings = BWSettings;
	type Output = Result<(), &'static str>;
	type State = BWState<T>;

	fn input_index(s: &mut Self::State) -> IndexOf<Self::Input> {
		s.elections
			.ongoing
			.clone()
			.into_iter()
			.map(|(height, extra)| (height, s.generate_election_properties_hook.run(height), extra))
			.collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, settings: &Self::Settings) -> Self::Output {
		// log::warn!("BW: input {i:?}");
		match i {
			SMInput::Context(ChainProgress::FirstConsensus(range)) => {
				s.elections.highest_election = range.start().saturating_backward(1);
				s.elections.schedule_range(range.clone());
				s.block_processor
					.process_block_data(ChainProgressInner::Progress(*range.start()), None);
			},

			SMInput::Context(ChainProgress::Range(range)) => {
				if *range.start() <= s.elections.highest_witnessed {
					//Reorg
					s.block_processor
						.process_block_data(ChainProgressInner::Reorg(range.clone()), None);
				} else {
					s.block_processor
						.process_block_data(ChainProgressInner::Progress(*range.end()), None);
				}
				s.elections.schedule_range(range);
			},

			SMInput::Context(ChainProgress::None) => {},

			SMInput::Vote(blockdata) => {
				s.elections.mark_election_done(blockdata.0 .0);
				log::info!("got block data: {:?}", blockdata.1);
				s.block_processor.process_block_data(
					ChainProgressInner::Progress(s.elections.highest_witnessed),
					Some((blockdata.0 .0, blockdata.1.data)),
				);
			},
		};

		s.elections.start_more_elections(
			settings.max_concurrent_elections as usize,
			s.safemode_enabled.run(()),
		);

		// log::warn!("BW: done. current elections: {:?}", s.elections.ongoing);

		Ok(())
	}

	/// Specifiation for step function
	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &Self::Input,
		settings: &Self::Settings,
		after: &Self::State,
	) {
		use itertools::Itertools;
		use ChainProgress::*;
		use SMInput::*;

		use crate::{asserts, electoral_systems::block_witnesser::helpers::*};

		let safemode_enabled = match before.safemode_enabled.run(()) {
			SafeModeStatus::Enabled => true,
			SafeModeStatus::Disabled => false,
		};

		assert!(
			// there should always be at most as many elections as given in the settings
			// or more if we had more elections previously
			after.elections.ongoing.len() <=
				sp_std::cmp::max(
					settings.max_concurrent_elections as usize,
					before.elections.ongoing.len()
				),
			"too many concurrent elections"
		);

		// all new elections use the current reorg id as index
		assert!(
			after.elections.ongoing.iter().all(|(height, ix)| {
				if !before.elections.ongoing.iter().contains(&(height, ix)) {
					*ix == after.elections.reorg_id
				} else {
					true
				}
			}),
			"new election with wrong index"
		);

		// ensure that as long as we are in safemode, `highest_priority` can increase only once
		asserts! {
			let could_increase = |s: &Self::State| s.elections.highest_election > s.elections.highest_priority;
			let is_first_consensus = matches!(input,Context(FirstConsensus(..)));
			let is_reorg = matches!(input, Context(Range(range)) if *range.start() <= before.elections.highest_election);
			let scheduled = |s: &Self::State| T::ChainBlockNumber::steps_between(&s.elections.highest_election,&s.elections.highest_priority).0 ;
			let outstanding = |s: &Self::State| scheduled(s) + s.elections.ongoing.len();

			"an increase of `highest_priority` can only happen if `could_increase` holds before"
			in (before.elections.highest_priority < after.elections.highest_priority).implies(could_increase(before));

			"if an increase happens, afterwards no increase can happen"
			in (before.elections.highest_priority < after.elections.highest_priority).implies(!could_increase(after));

			"if no increase can happen, then as long as we have safemode, it can't happen in the future as well"
			in (!could_increase(before) && safemode_enabled && !is_first_consensus).implies(!could_increase(after));

			"if safemode is enabled, we don't have a reorg, we aren't getting a first consensus and we can't increase then the number of outstanding elections doesn't increase"
			in (safemode_enabled && !could_increase(before) && !is_reorg && !is_first_consensus).implies(outstanding(before) >= outstanding(after));

			"if outstanding is 0 and we are in safemode, then there are no elections ongoing"
			in (safemode_enabled && outstanding(before) == 0).implies(after.elections.ongoing.is_empty() && outstanding(after) == 0);

		}

		// TODO: make sure that the number of outstanding elections does not grow

		match input {
			Vote(MultiIndexAndValue((height, _, _), _)) => {
				let highest_election = if safemode_enabled {
					before.elections.highest_priority
				} else {
					before.elections.highest_witnessed
				};

				let new_elections = (before.elections.highest_election.saturating_forward(1)..=
					highest_election)
					.take(
						(settings.max_concurrent_elections as usize + 1)
							.saturating_sub(before.elections.ongoing.len()),
					)
					.collect();

				// the elections after a vote are the ones from before, minus the voted one + all
				// outstanding ones
				let after_should =
					before.elections.ongoing.key_set().without(*height).merge(new_elections);

				assert_eq!(
					after.elections.ongoing.key_set(),
					after_should,
					"wrong ongoing election set after received vote",
				)
			},

			Context(Range(range) | FirstConsensus(range)) => {
				// we always track the highest seen block
				assert_eq!(
					after.elections.highest_witnessed,
					std::cmp::max(*range.end(), before.elections.highest_witnessed),
					"the highest seen block should always be tracked"
				);

				// if the input is `FirstConsensus`, we use the beginning of the range
				// as the first block we ought to emit an election for.
				let before_highest_election = if let Context(FirstConsensus(_)) = input {
					range.start().saturating_backward(1)
				} else {
					before.elections.highest_election
				};

				// if !safemode_enabled {
				if *range.start() <= before_highest_election {
					assert!(
							!before.elections.ongoing.values().contains(&after.elections.reorg_id),
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
								after.elections.ongoing.get(height).is_none_or(|index| *index == after.elections.reorg_id),
								"ongoing election which was part of reorg should either be removed or stay open with new index (after.ongoing = {:?})", after.elections.ongoing
							)
					}
				}
				// } else {
				// if safe mode is enabled, ongoing elections shouldn't change
				// assert!(
				// 	before.elections.ongoing == after.elections.ongoing,
				// 	"if safemode is enabled, ongoing elections shouldn't change (except being closed
				// when they come to consensus)" );

				// but the next_election should still be updated in order to restart the
				// reorg'ed elections after safemode is disabled
				// if *range.start() < before_next_election {
				// 	// assert!(
				// 	// 	after.elections.next_election == *range.start(),
				// 	// 	"if safemode is enabled, and a reorg happens, next_election should be the
				// start of the reorg range" 	// );
				// } else {
				// 	assert!(
				// 		after.elections.next_election == before_next_election,
				// 		"if safemode is enabled, and no (relevant) reorg happens, next_election should
				// not be udpated" 	);
				// }
				// }
			},

			Context(None) => (),
		}
	}
}

#[cfg(test)]
mod tests {
	// use std::collections::BTreeMap;
	//
	// use cf_chains::{witness_period::BlockWitnessRange, ChainWitnessConfig};
	// use proptest::{
	// 	prelude::{any, Arbitrary, BoxedStrategy, Just, Strategy},
	// 	strategy::LazyJust,
	// };
	//
	// use super::*;
	// use crate::prop_do;
	// use hook_test_utils::*;
	//
	// fn generate_state<T: BWTypes<SafeModeEnabledHook = ConstantHook<(), SafeModeStatus>>>(
	// ) -> impl Strategy<Value = BWState<T>>
	// where
	// 	T::ChainBlockNumber: Arbitrary,
	// 	T::ElectionPropertiesHook: Default + Clone + Debug + Eq , <T as BWTypes>::BlockProcessor:
	// Default, {
	// 	prop_do! {
	// 		// let next_election in any::<T::ChainBlockNumber>();
	// 		// let highest_scheduled in any::<T::ChainBlockNumber>();
	// 		// let highest_started_and_touched_by_reorg in any::<T::ChainBlockNumber>();
	// 		// let reorg_id in any::<u8>();
	// 		// let safemode_enabled in any::<bool>().prop_map(|b| if b {SafeModeStatus::Enabled} else
	// {SafeModeStatus::Disabled});
	//
	// 		let (highest_election, highest_scheduled, highest_started_and_touched_by_reorg, reorg_id,
	// safemode_enabled) in ( 			// Just(T::ChainBlockNumber::zero()),
	// 			any::<T::ChainBlockNumber>(),
	// 			any::<T::ChainBlockNumber>(),
	// 			any::<T::ChainBlockNumber>(),
	// 			any::<u8>(),
	// 			any::<bool>().prop_map(|b| if b {SafeModeStatus::Enabled} else {SafeModeStatus::Disabled})
	// 		);
	//
	// 		// let ongoing in prop::collection::vec((any::<T::ChainBlockNumber>(), any::<u8>()),
	// 0..10).prop_map(move |xs| xs.into_iter().filter(move |(height, _)| *height < next_election));
	// 		LazyJust::new(move || BWState {
	// 			elections: ElectionTracker {
	// 				highest_election: highest_election.clone(),
	// 				highest_witnessed: highest_scheduled.clone(),
	// 				highest_priority: highest_started_and_touched_by_reorg.clone(),
	// 				ongoing: BTreeMap::new(), // BTreeMap::from_iter(ongoing.clone()),
	// 				reorg_id
	// 			},
	// 			generate_election_properties_hook: Default::default(),
	// 			safemode_enabled: ConstantHook::new(safemode_enabled),
	// 			block_processor: Default::default(),
	// 			_phantom: core::marker::PhantomData,
	// 		})
	// 	}
	//
	// 	// Just(
	// 	// 	BWState {
	// 	// 	elections: ElectionTracker {
	// 	// 		next_election: BlockZero::zero(),
	// 	// 		highest_seen: BlockZero::zero(),
	// 	// 		highest_priority: BlockZero::zero(),
	// 	// 		ongoing: BTreeMap::new(),
	// 	// 		reorg_id: 0,
	// 	// 	},
	// 	// 	generate_election_properties_hook: Default::default(),
	// 	// 	safemode_enabled: ConstantHook::new(SafeModeStatus::Disabled),
	// 	// 	_phantom: core::marker::PhantomData,
	// 	// }
	// 	// )
	// }
	//
	// fn generate_input<T: BWTypes<BlockData = ()>>(
	// 	indices: IndexOf<<BWStateMachine<T> as StateMachine>::Input>,
	// ) -> BoxedStrategy<<BWStateMachine<T> as StateMachine>::Input>
	// where
	// 	T::ChainBlockNumber: Arbitrary,
	// {
	// 	/*
	// 	let generate_input = |index| {
	// 		prop_oneof![
	// 			Just(SMInput::Vote(MultiIndexAndValue(index, ConstantIndex::new(())))),
	// 			prop_oneof![
	// 				Just(ChainProgress::None),
	// 				(any::<T::ChainBlockNumber>(), 0..20usize)
	// 					.prop_map(|(a, b)| ChainProgress::Range(a..=a.saturating_forward(b))),
	// 				(any::<T::ChainBlockNumber>(), 0..20usize).prop_map(|(a, b)| {
	// 					ChainProgress::FirstConsensus(a..=a.saturating_forward(b))
	// 				})
	// 			]
	// 			.prop_map(SMInput::Context)
	// 		]
	// 	};
	//
	// 	if indices.len() > 0 {
	// 		prop_do! {
	// 			let index in select(indices);
	// 			generate_input(index.clone())
	// 			// return SMInput::Vote(MultiIndexAndValue(index, ConstantIndex::new(input)))
	// 		}
	// 		.boxed()
	// 	} else {
	// 		Just(SMInput::Context(ChainProgress::None)).boxed()
	// 	}
	// 	*/
	//
	// 	Just(SMInput::Context(ChainProgress::None)).boxed()
	// }
	//
	// // impl<N: Serde + Copy + Ord + SaturatingStep + Step + BlockZero + Debug + 'static> BWTypes
	// for N { // 	type ChainBlockNumber = N;
	// // 	type BlockData = ();
	// // 	type ElectionProperties = ();
	// // 	type ElectionPropertiesHook = ConstantHook<N, ()>;
	// // 	type SafeModeEnabledHook = ConstantHook<(), SafeModeStatus>;
	// // 	type BWProcessorTypes = ();
	// // 	type BlockProcessor = ();
	// // }
	//
	// // #[test]
	// // pub fn test_bw_statemachine() {
	// // 	BWStateMachine::<u8>::test(
	// // 		file!(),
	// // 		generate_state(),
	// // 		prop_do! {
	// // 			let max_concurrent_elections in 0..10u16;
	// // 			return BWSettings { max_concurrent_elections }
	// // 		},
	// // 		generate_input::<u8>,
	// // 	);
	// // }
	//
	// struct TestChain {}
	// impl ChainWitnessConfig for TestChain {
	// 	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
	// 	type ChainBlockNumber = u32;
	// }
	//
	// // #[test]
	// // pub fn test_bw_statemachine2() {
	// // 	BWStateMachine::<BlockWitnessRange<TestChain>>::test(
	// // 		file!(),
	// // 		generate_state(),
	// // 		prop_do! {
	// // 			let max_concurrent_elections in 0..10u16;
	// // 			return BWSettings { max_concurrent_elections }
	// // 		},
	// // 		generate_input::<BlockWitnessRange<TestChain>>,
	// // 	);
	// // }
}
