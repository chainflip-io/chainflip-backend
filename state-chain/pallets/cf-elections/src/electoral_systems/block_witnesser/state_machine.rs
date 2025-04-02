use super::{
	super::state_machine::core::*,
	primitives::{ElectionTracker, SafeModeStatus},
};
use crate::electoral_systems::{
	block_height_tracking::ChainProgress,
	block_witnesser::{block_processor::BlockProcessor, primitives::ChainProgressInner},
	state_machine::{core::Validate, state_machine::Statemachine, state_machine_es::SMInput},
};
use cf_chains::witness_period::{BlockZero, SaturatingStep};
use codec::{Decode, Encode};
use core::{iter::Step, ops::Range};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, vec::Vec};

/// Type which can be used for implementing traits that
/// contain only type definitions, as used in many parts of
/// the state machine based electoral systems.
#[derive_where(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound())]
#[serde(bound = "")]
#[scale_info(skip_type_params(Tag1, Tag2))]
pub struct HookTypeFor<Tag1, Tag2> {
	_phantom: sp_std::marker::PhantomData<(Tag1, Tag2)>,
}

pub trait BWTypes: 'static + Sized + BWProcessorTypes {
	type ElectionProperties: PartialEq + Clone + Eq + Debug + 'static;
	type ElectionPropertiesHook: Hook<HookTypeFor<Self, ElectionPropertiesHook>>;
	type SafeModeEnabledHook: Hook<HookTypeFor<Self, SafeModeEnabledHook>>;
}

// hook types
pub struct SafeModeEnabledHook;
impl<T: BWTypes> HookType for HookTypeFor<T, SafeModeEnabledHook> {
	type Input = ();
	type Output = SafeModeStatus;
}

pub struct ElectionPropertiesHook;
impl<T: BWTypes> HookType for HookTypeFor<T, ElectionPropertiesHook> {
	type Input = T::ChainBlockNumber;
	type Output = T::ElectionProperties;
}

pub struct RulesHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, RulesHook> {
	type Input = (T::ChainBlockNumber, Range<u32>, T::BlockData, u32);
	type Output = Vec<(T::ChainBlockNumber, T::Event)>;
}

pub struct ExecuteHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, ExecuteHook> {
	type Input = Vec<(T::ChainBlockNumber, T::Event)>;
	type Output = ();
}

pub trait BWProcessorTypes: Sized {
	type ChainBlockNumber: Serde
		+ Copy
		+ Eq
		+ Ord
		+ SaturatingStep
		+ Step
		+ BlockZero
		+ Debug
		+ 'static;
	type BlockData: PartialEq + Clone + Debug + Eq + Serde + 'static;

	type Event: Serde + Debug + Clone + Eq;
	type Rules: Hook<HookTypeFor<Self, RulesHook>> + Default + Serde + Debug + Clone + Eq;
	type Execute: Hook<HookTypeFor<Self, ExecuteHook>> + Default + Serde + Debug + Clone + Eq;
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
pub struct BlockWitnesserSettings {
	pub max_concurrent_elections: u16,
	pub safety_margin: u32,
}

#[derive_where(Debug, Clone, PartialEq, Eq;
	T::SafeModeEnabledHook: Debug + Clone + Eq,
	T::ElectionPropertiesHook: Debug + Clone + Eq,
	BlockProcessor<T>: Debug + Clone + Eq,
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::ElectionPropertiesHook: Encode,
	T::SafeModeEnabledHook: Encode,

	BlockProcessor<T>: Encode,
))]
pub struct BlockWitnesserState<T: BWTypes> {
	pub elections: ElectionTracker<T::ChainBlockNumber>,
	pub generate_election_properties_hook: T::ElectionPropertiesHook,
	pub safemode_enabled: T::SafeModeEnabledHook,
	pub block_processor: BlockProcessor<T>,
	pub _phantom: sp_std::marker::PhantomData<T>,
}

impl<T: BWTypes> Validate for BlockWitnesserState<T> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.elections.is_valid()
	}
}

impl<T: BWTypes> Default for BlockWitnesserState<T>
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

#[derive_where(Debug, Clone, PartialEq, Eq;
	T::ChainBlockNumber: Debug + Clone + Eq,
	T::ElectionProperties: Debug + Clone + Eq,
)]
#[derive_where(Default;
	T::ChainBlockNumber: Default,
	T::ElectionProperties: Default,
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::ElectionProperties: Encode,
))]
pub struct BWElectionProperties<T: BWTypes> {
	pub block_height: T::ChainBlockNumber,
	pub properties: T::ElectionProperties,
	pub reorg_id: u8,
}

impl<T: BWTypes> IndexedValidate<BWElectionProperties<T>, T::BlockData> for BWStatemachine<T> {
	type Error = ();

	fn validate(
		_index: &BWElectionProperties<T>,
		_value: &T::BlockData,
	) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[derive(Debug)]
pub struct BWStatemachine<Types: BWTypes> {
	_phantom: sp_std::marker::PhantomData<Types>,
}

impl<T: BWTypes> Statemachine for BWStatemachine<T> {
	type Input =
		SMInput<(BWElectionProperties<T>, T::BlockData), ChainProgress<T::ChainBlockNumber>>;
	type InputIndex = Vec<BWElectionProperties<T>>;
	type Settings = BlockWitnesserSettings;
	type Output = Result<(), &'static str>;
	type State = BlockWitnesserState<T>;

	fn input_index(s: &mut Self::State) -> Self::InputIndex {
		s.elections
			.ongoing
			.clone()
			.into_iter()
			.map(|(block_height, reorg_id)| BWElectionProperties {
				properties: s.generate_election_properties_hook.run(block_height),
				block_height,
				reorg_id,
			})
			.collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, settings: &Self::Settings) -> Self::Output {
		// log::warn!("BW: input {i:?}");
		match i {
			SMInput::Context(ChainProgress::FirstConsensus(range)) => {
				s.elections.next_election = *range.start();
				s.elections.schedule_range(range.clone());
				s.block_processor
					.process_block_data(ChainProgressInner::Progress(*range.start()), None);
			},

			SMInput::Context(ChainProgress::Range(range)) => {
				if *range.start() < s.elections.next_witnessed {
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

			SMInput::Consensus((properties, blockdata)) => {
				s.elections.mark_election_done(properties.block_height);
				log::info!("got block data: {:?}", blockdata);
				s.block_processor.process_block_data(
					ChainProgressInner::Progress(s.elections.next_witnessed.saturating_backward(1)),
					Some((properties.block_height, blockdata, settings.safety_margin)),
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
			let could_increase = |s: &Self::State| s.elections.next_election > s.elections.next_priority_election;
			let is_first_consensus = matches!(input,Context(FirstConsensus(..)));
			let is_reorg = matches!(input, Context(Range(range)) if *range.start() <= before.elections.next_election);
			let scheduled = |s: &Self::State| T::ChainBlockNumber::steps_between(&s.elections.next_election,&s.elections.next_priority_election).0 ;
			let outstanding = |s: &Self::State| scheduled(s) + s.elections.ongoing.len();

			"an increase of `highest_priority` can only happen if `could_increase` holds before"
			in (before.elections.next_priority_election < after.elections.next_priority_election).implies(could_increase(before));

			"if safemode is enabled, if an increase happens, afterwards no increase can happen"
			in (before.elections.next_priority_election < after.elections.next_priority_election && safemode_enabled).implies(!could_increase(after));

			"if no increase can happen, then as long as we have safemode, it can't happen in the future as well"
			in (!could_increase(before) && safemode_enabled && !is_first_consensus).implies(!could_increase(after));

			"if safemode is enabled, we don't have a reorg, we aren't getting a first consensus and we can't increase then the number of outstanding elections doesn't increase"
			in (safemode_enabled && !could_increase(before) && !is_reorg && !is_first_consensus).implies(outstanding(before) >= outstanding(after));

			"if outstanding is 0 and we are in safemode, then there are no elections ongoing"
			in (safemode_enabled && outstanding(before) == 0).implies(after.elections.ongoing.is_empty() && outstanding(after) == 0);

		}

		// TODO: make sure that the number of outstanding elections does not grow

		match input {
			Consensus((BWElectionProperties { block_height: height, .. }, _)) => {
				let next_election = if safemode_enabled {
					before.elections.next_priority_election
				} else {
					before.elections.next_witnessed
				};

				let new_elections = (before.elections.next_election..next_election)
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
					after.elections.next_witnessed,
					std::cmp::max(
						range.end().saturating_forward(1),
						before.elections.next_witnessed
					),
					"the highest seen block should always be tracked"
				);

				// if the input is `FirstConsensus`, we use the beginning of the range
				// as the first block we ought to emit an election for.
				let before_next_election = if let Context(FirstConsensus(_)) = input {
					*range.start()
				} else {
					before.elections.next_election
				};

				// if !safemode_enabled {
				if *range.start() < before_next_election {
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
			},

			Context(None) => (),
		}
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use cf_chains::{witness_period::BlockWitnessRange, ChainWitnessConfig};
	use proptest::{
		prelude::{any, Arbitrary, BoxedStrategy, Just, Strategy},
		prop_oneof,
		sample::select,
		strategy::LazyJust,
	};

	use super::*;
	use crate::prop_do;
	use hook_test_utils::*;

	const SAFETY_MARGIN: u32 = 3;
	fn generate_state<
		T: BWTypes<SafeModeEnabledHook = MockHook<HookTypeFor<T, SafeModeEnabledHook>>>,
	>() -> impl Strategy<Value = BlockWitnesserState<T>>
	where
		T::ChainBlockNumber: Arbitrary,
		T::ElectionPropertiesHook: Default + Clone + Debug + Eq,
		T::BlockData: Default + Clone + Debug + Eq,
	{
		prop_do! {
			let (next_election, next_witnessed,
				 next_priority_election, reorg_id,
				safemode_enabled) in (
				any::<T::ChainBlockNumber>(),
				any::<T::ChainBlockNumber>(),
				any::<T::ChainBlockNumber>(),
				any::<u8>(),
				any::<bool>().prop_map(|b| if b {SafeModeStatus::Enabled} else {SafeModeStatus::Disabled})
			);

			let ongoing in proptest::collection::vec((any::<T::ChainBlockNumber>(), any::<u8>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move |(height, _)| *height < next_election));
			LazyJust::new(move || BlockWitnesserState {
				elections: ElectionTracker {
					next_election,
					next_witnessed,
					next_priority_election,
					ongoing: BTreeMap::from_iter(ongoing.clone()),
					reorg_id
				},
				generate_election_properties_hook: Default::default(),
				safemode_enabled: MockHook::new(safemode_enabled),
				block_processor: BlockProcessor {
					blocks_data: Default::default(),
					reorg_events: Default::default(),
					rules: Default::default(),
					execute: Default::default(),
				},
				_phantom: core::marker::PhantomData,
			})
		}
	}

	fn generate_input<T: BWTypes<BlockData = ()>>(
		indices: <BWStatemachine<T> as Statemachine>::InputIndex,
	) -> BoxedStrategy<<BWStatemachine<T> as Statemachine>::Input>
	where
		T::ChainBlockNumber: Arbitrary,
	{
		let generate_input = |index| {
			prop_oneof![
				Just(SMInput::Consensus((index, ()))),
				prop_oneof![
					Just(ChainProgress::None),
					(any::<T::ChainBlockNumber>(), 0..20usize)
						.prop_map(|(a, b)| ChainProgress::Range(a..=a.saturating_forward(b))),
					(any::<T::ChainBlockNumber>(), 0..20usize).prop_map(|(a, b)| {
						ChainProgress::FirstConsensus(a..=a.saturating_forward(b))
					})
				]
				.prop_map(SMInput::Context)
			]
		};

		if indices.len() > 0 {
			prop_do! {
				let index in select(indices);
				generate_input(index.clone())
			}
			.boxed()
		} else {
			Just(SMInput::Context(ChainProgress::None)).boxed()
		}
	}

	impl<N: Serde + Copy + Ord + SaturatingStep + Step + BlockZero + Debug + Default + 'static>
		BWProcessorTypes for N
	{
		type ChainBlockNumber = N;
		type BlockData = ();
		type Event = ();
		type Rules = MockHook<HookTypeFor<Self, RulesHook>>;
		type Execute = MockHook<HookTypeFor<Self, ExecuteHook>>;
	}

	impl<N: Serde + Copy + Ord + SaturatingStep + Step + BlockZero + Debug + Default + 'static>
		BWTypes for N
	{
		type ElectionProperties = ();
		type ElectionPropertiesHook = MockHook<HookTypeFor<Self, ElectionPropertiesHook>>;
		type SafeModeEnabledHook = MockHook<HookTypeFor<Self, SafeModeEnabledHook>>;
	}

	#[test]
	pub fn test_bw_statemachine() {
		BWStatemachine::<u32>::test(
			file!(),
			generate_state(),
			prop_do! {
				let max_concurrent_elections in 0..10u16;
				return BlockWitnesserSettings { max_concurrent_elections, safety_margin: SAFETY_MARGIN}
			},
			generate_input::<u32>,
		);
	}

	struct TestChain {}
	impl ChainWitnessConfig for TestChain {
		const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
		type ChainBlockNumber = u32;
	}

	#[test]
	pub fn test_bw_statemachine2() {
		BWStatemachine::<BlockWitnessRange<TestChain>>::test(
			file!(),
			generate_state(),
			prop_do! {
				let max_concurrent_elections in 0..10u16;
				return BlockWitnesserSettings { max_concurrent_elections, safety_margin: SAFETY_MARGIN}
			},
			generate_input::<BlockWitnessRange<TestChain>>,
		);
	}
}
