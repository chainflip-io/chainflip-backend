use super::{
	super::state_machine::core::*,
	block_processor::BlockProcessorEvent,
	primitives::{ElectionTracker, ElectionTrackerEvent, SafeModeStatus},
};
#[cfg(test)]
use crate::electoral_systems::state_machine::state_machine::InputOf;
use crate::electoral_systems::{
	block_height_tracking::{
		ChainBlockHashOf, ChainBlockNumberOf, ChainProgress, ChainTypes, CommonTraits,
		MaybeArbitrary, TestTraits,
	},
	block_witnesser::block_processor::BlockProcessor,
	state_machine::{
		core::Validate,
		state_machine::{AbstractApi, Statemachine},
	},
};
use codec::{Decode, Encode};
use core::ops::Range;
use derive_where::derive_where;
use itertools::Either;
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
	type ElectionProperties: MaybeArbitrary + CommonTraits + TestTraits;
	type ElectionPropertiesHook: Hook<HookTypeFor<Self, ElectionPropertiesHook>> + CommonTraits;
	type SafeModeEnabledHook: Hook<HookTypeFor<Self, SafeModeEnabledHook>> + CommonTraits;

	type ElectionTrackerDebugEventHook: Hook<HookTypeFor<Self, ElectionTrackerDebugEventHook>>
		+ CommonTraits
		+ TestTraits
		+ Default;
}

// hook types
pub struct ElectionTrackerDebugEventHook;
impl<T: BWTypes> HookType for HookTypeFor<T, ElectionTrackerDebugEventHook> {
	type Input = ElectionTrackerEvent<T>;
	type Output = ();
}

pub struct SafeModeEnabledHook;
impl<T: BWTypes> HookType for HookTypeFor<T, SafeModeEnabledHook> {
	type Input = ();
	type Output = SafeModeStatus;
}

pub struct ElectionPropertiesHook;
impl<T: BWTypes> HookType for HookTypeFor<T, ElectionPropertiesHook> {
	type Input = ChainBlockNumberOf<T::Chain>;
	type Output = T::ElectionProperties;
}

pub struct RulesHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, RulesHook> {
	type Input = (Range<u32>, T::BlockData, u32);
	type Output = Vec<T::Event>;
}

pub struct ExecuteHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, ExecuteHook> {
	type Input = Vec<(ChainBlockNumberOf<T::Chain>, T::Event)>;
	type Output = ();
}

pub struct DebugEventHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, DebugEventHook> {
	type Input = BlockProcessorEvent<T>;
	type Output = ();
}

pub trait BlockDataTrait = CommonTraits + TestTraits + MaybeArbitrary + Ord + 'static;
pub trait BWProcessorTypes: Sized + Debug + Clone + Eq {
	type Chain: ChainTypes;
	type BlockData: BlockDataTrait;

	type Event: CommonTraits + Ord + Encode;
	type Rules: Hook<HookTypeFor<Self, RulesHook>> + Default + CommonTraits;
	type Execute: Hook<HookTypeFor<Self, ExecuteHook>> + Default + CommonTraits;

	type DebugEventHook: Hook<HookTypeFor<Self, DebugEventHook>> + Default + CommonTraits;
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
	pub max_ongoing_elections: u16,
	pub safety_margin: u32,
}

defx! {
	#[derive(Default)]
	pub struct BlockWitnesserState[T: BWTypes] {
		pub elections: ElectionTracker<T>,
		pub generate_election_properties_hook: T::ElectionPropertiesHook,
		pub safemode_enabled: T::SafeModeEnabledHook,
		pub block_processor: BlockProcessor<T>,
	}
	validate _this (else BlockWitnesserError) {}
}

def_derive!(
	pub enum EngineElectionType<C: ChainTypes> {
		ByHash(C::ChainBlockHash),
		BlockHeight { submit_hash: bool },
	}
);
def_derive! {
	#[no_serde]
	pub struct BWElectionProperties<T: BWTypes> {
		pub election_type: EngineElectionType<T::Chain>,
		pub block_height: ChainBlockNumberOf<T::Chain>,
		pub properties: T::ElectionProperties,
	}
}
impl<T: BWTypes> Validate for BWElectionProperties<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

defx! {
	#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
	pub enum BWElectionType[T: BWTypes] {
		/// Querying blocks we haven't received a hash for yet
		Optimistic,

		/// Querying blocks by hash
		ByHash(<<T as BWProcessorTypes>::Chain as ChainTypes>::ChainBlockHash),

		/// Querying "old" blocks that are below the safety margin,
		/// and thus we don't care about the hash anymore
		SafeBlockHeight,

		Governance(T::ElectionProperties),
	}
	validate _this (else BWElectionTypeError) {}
}

#[derive(Debug)]
pub struct BWStatemachine<Types: BWTypes> {
	_phantom: sp_std::marker::PhantomData<Types>,
}

impl<T: BWTypes> AbstractApi for BWStatemachine<T> {
	type Query = BWElectionProperties<T>;
	type Response = (T::BlockData, Option<ChainBlockHashOf<T::Chain>>);
	type Error = ();

	fn validate(
		index: &BWElectionProperties<T>,
		(_, hash): &(T::BlockData, Option<ChainBlockHashOf<T::Chain>>),
	) -> Result<(), Self::Error> {
		use EngineElectionType::*;
		// ensure that a hash is only provided for `Optimistic` elections.
		match (&index.election_type, hash) {
			(ByHash(_), None) |
			(BlockHeight { submit_hash: true }, Some(_)) |
			(BlockHeight { submit_hash: false }, None) => Ok(()),
			_ => Err(()),
		}
	}
}

impl<T: BWTypes> Statemachine for BWStatemachine<T> {
	type Context = Option<ChainProgress<T::Chain>>;
	type Settings = BlockWitnesserSettings;
	type Output = Result<(), &'static str>;
	type State = BlockWitnesserState<T>;

	fn input_index(state: &mut Self::State) -> Vec<Self::Query> {
		state
			.elections
			.ongoing
			.clone()
			.into_iter()
			.map(|(block_height, election_type)| match election_type {
				BWElectionType::Governance(properties) => BWElectionProperties {
					properties,
					election_type: EngineElectionType::BlockHeight { submit_hash: false },
					block_height,
				},
				BWElectionType::Optimistic => BWElectionProperties {
					properties: state.generate_election_properties_hook.run(block_height),
					election_type: EngineElectionType::BlockHeight { submit_hash: true },
					block_height,
				},
				BWElectionType::SafeBlockHeight => BWElectionProperties {
					properties: state.generate_election_properties_hook.run(block_height),
					election_type: EngineElectionType::BlockHeight { submit_hash: false },
					block_height,
				},
				BWElectionType::ByHash(hash) => BWElectionProperties {
					properties: state.generate_election_properties_hook.run(block_height),
					election_type: EngineElectionType::ByHash(hash),
					block_height,
				},
			})
			.collect()
	}

	fn step(
		state: &mut Self::State,
		input: Either<Self::Context, (Self::Query, Self::Response)>,
		settings: &Self::Settings,
	) -> Self::Output {
		match input {
			Either::Left(Some(progress)) => {
				if let Some(ref removed_block_heights) = progress.removed {
					state.block_processor.process_reorg(
						state.elections.seen_heights_below,
						removed_block_heights.clone(),
					);
				}
				for (height, accepted_optimistic_block) in state.elections.schedule_range(progress)
				{
					state.block_processor.insert_block_data(
						height,
						accepted_optimistic_block.data,
						settings.safety_margin,
					);
				}
			},

			Either::Left(None) => {},

			Either::Right((properties, (blockdata, blockhash))) => {
				if let Some(blockdata) = state.elections.mark_election_done(
					properties.block_height,
					&properties.election_type,
					&blockhash,
					blockdata,
				) {
					state.block_processor.insert_block_data(
						properties.block_height,
						blockdata,
						settings.safety_margin,
					);
				}
			},
		};

		state.block_processor.process_blocks_up_to(
			state.elections.seen_heights_below,
			// NOTE: we use the lowest "in progress" height for expiring block and event
			// data, this way if one BW election is stuck (e.g. due to failing rpc
			// call), no event data is going to be deleted. That's why we can't use
			// the `highest_seen` block height, because that one progresses always
			// following data from the BHW, ignoring ongoing elections.
			state.elections.lowest_in_progress_height(),
		);

		state.elections.start_more_elections(
			settings.max_ongoing_elections as usize,
			state.safemode_enabled.run(()),
		);

		Ok(())
	}

	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &InputOf<Self>,
		_output: &Self::Output,
		settings: &Self::Settings,
		after: &Self::State,
	) {
		use crate::electoral_systems::state_machine::test_utils::{BTreeMultiSet, Container};
		use cf_chains::witness_period::SaturatingStep;
		use std::collections::BTreeSet;

		// there should always be at most as many elections as given in the settings
		// or more if we had more elections previously
		assert!(
			after.elections.ongoing.len() <=
				sp_std::cmp::max(
					settings.max_ongoing_elections as usize,
					before.elections.ongoing.len()
				),
			"too many concurrent elections"
		);

		if before.safemode_enabled.run(()) == SafeModeStatus::Enabled {
			assert_eq!(
				before.elections.highest_ever_ongoing_election,
				after.elections.highest_ever_ongoing_election,
				"during safemode, no higher elections should be scheduled than heights that were scheduled before"
			);
		}

		// Every block height is in a single state and isn't lost until it expires
		let get_all_heights = |s: &Self::State| {
			s.elections
				.ongoing
				.iter()
				.filter(|(_, t)| **t != BWElectionType::Optimistic)
				.map(|(h, _)| h)
				.cloned()
				.chain(s.elections.queued_hash_elections.keys().cloned())
				.chain(s.elections.queued_safe_elections.get_all_heights().into_iter())
				.chain(s.block_processor.blocks_data.keys().cloned())
				.collect::<Vec<_>>()
		};

		let counted_heights: Container<BTreeMultiSet<_>> =
			get_all_heights(&after).into_iter().collect();

		// we have unique heights
		for (height, count) in counted_heights.0 .0.clone() {
			if count > 1 {
				panic!("Got height {height:?} in total {count} times");
			}
		}

		let (new_heights, removed_heights) = match input {
			Either::Left(Some(progress)) => (
				progress
					.headers
					.headers
					.iter()
					.map(|block| block.block_height)
					.collect::<Vec<_>>(),
				progress.removed.clone(),
			),
			Either::Left(None) => Default::default(),
			Either::Right(_) => Default::default(),
		};

		assert_eq!(
			get_all_heights(&before)
				.into_iter()
				.filter(|h| *h < after.elections.seen_heights_below)
				.filter(|h| !removed_heights.iter().any(|range| range.contains(h)))
				.chain(new_heights)
				.filter(|h| h.saturating_forward(T::Chain::SAFETY_BUFFER) >=
					after.elections.lowest_in_progress_height())
				.collect::<BTreeSet<_>>(),
			get_all_heights(&after)
				.into_iter()
				.filter(|h| *h < after.elections.seen_heights_below)
				.collect::<BTreeSet<_>>(),
			"wrong set of heights, before: {before:#?}\n, after {after:#?}\n, input {input:#?}\n, lowest_in_progress (before: {:?}, after: {:?})",
			before.elections.lowest_in_progress_height(),
			after.elections.lowest_in_progress_height(),
		);
	}
}

#[cfg(test)]
pub mod tests {
	// use std::collections::BTreeMap;

	use core::ops::RangeInclusive;

	use proptest::{
		arbitrary::{self, arbitrary, arbitrary_with},
		prelude::{any, Arbitrary, BoxedStrategy, Just, Strategy},
		prop_oneof,
		strategy::LazyJust,
	};

	use super::*;
	use crate::{
		electoral_systems::{
			block_height_tracking::{
				self, primitives::NonemptyContinuousHeaders, BHWTypes, ChainBlockHashTrait,
				ChainBlockNumberTrait, HeightWitnesserProperties,
			},
			block_witnesser::primitives::CompactHeightTracker,
		},
		prop_do,
	};
	use frame_support::sp_runtime::offchain::http::Response;
	use hook_test_utils::*;

	// const SAFETY_MARGIN: u32 = 3;
	fn generate_state<
		T: BWTypes<SafeModeEnabledHook = MockHook<HookTypeFor<T, SafeModeEnabledHook>>>,
	>() -> impl Strategy<Value = BlockWitnesserState<T>>
	where
		T::ElectionPropertiesHook: Default,
		T::BlockData: Default,
	{
		(any::<SafeModeStatus>(), any::<ElectionTracker<T>>()).prop_map(|(safemode, elections)| {
			BlockWitnesserState {
				elections,
				generate_election_properties_hook: Default::default(),
				safemode_enabled: MockHook::new(safemode),
				block_processor: Default::default(),
			}
		})
		/*
			   prop_do! {
				   let (next_election, seen_heights_below,
						priority_elections_below,
					   safemode_enabled) in (
					   any::<ChainBlockNumberOf<T::Chain>>(),
					   any::<ChainBlockNumberOf<T::Chain>>(),
					   any::<ChainBlockNumberOf<T::Chain>>(),
					   any::<bool>().prop_map(|b| if b {SafeModeStatus::Enabled} else {SafeModeStatus::Disabled})
				   );

				   let (ongoing, queued_elections) in
				   (
					   proptest::collection::vec((any::<ChainBlockNumberOf<T::Chain>>(),
		   any::<BWElectionType<T::Chain>>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move
		   |(height, _)| *height < next_election)),
					   proptest::collection::vec((any::<ChainBlockNumberOf<T::Chain>>(),
		   any::<ChainBlockHashOf<T::Chain>>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move
		   |(height, _)| *height < next_election)) 		);
				   LazyJust::new(move || BlockWitnesserState {
					   elections: ElectionTracker {
						   // queued_next_safe_height: None,
						   queued_elections: BTreeMap::from_iter(queued_elections.clone()),
						   seen_heights_below,
						   priority_elections_below,
						   ongoing: BTreeMap::from_iter(ongoing.clone()),
						   queued_safe_elections: Default::default(),
						   optimistic_block_cache: Default::default(),
						   debug_events: Default::default()
					   },
					   generate_election_properties_hook: Default::default(),
					   safemode_enabled: MockHook::new(safemode_enabled),
					   block_processor: BlockProcessor {
						   blocks_data:Default::default(),
						   processed_events:Default::default(),
						   rules:Default::default(),
						   execute:Default::default(),
						   debug_events: Default::default()
					   },
				   })
			   }
		*/
	}

	fn generate_context<T: BWTypes>(
		state: &BlockWitnesserState<T>,
	) -> BoxedStrategy<Option<ChainProgress<T::Chain>>>
	where
		T::Chain: Arbitrary,
	{
		let safe_block_height =
			state.elections.seen_heights_below.saturating_backward(T::Chain::SAFETY_BUFFER);
		any::<Option<ChainProgress<T::Chain>>>()
			.prop_map(|progress| {
				progress.map(|mut progress| {
					if let Some(removed) = &mut progress.removed {
						*removed = *removed.start()..=
							core::cmp::min(*removed.end(), removed.start().saturating_forward(10));
					}
					progress
				})
			})
			.prop_filter("whence", move |progress| {
				if let Some(progress) = progress {
					progress
						.headers
						.headers
						.iter()
						.all(|header| header.block_height > safe_block_height)
				} else {
					true
				}
			})
			.boxed()
	}

	fn generate_settings(
	) -> impl Strategy<Value = BlockWitnesserSettings> + Clone + Debug + Sync + Send {
		prop_do! {
			let max_ongoing_elections in 1..10u16;
			let safety_margin in 1..5u32;
			return BlockWitnesserSettings { safety_margin, max_ongoing_elections }
		}
	}

	fn generate_input<T: BWTypes>(
		index: <BWStatemachine<T> as AbstractApi>::Query,
	) -> BoxedStrategy<<BWStatemachine<T> as AbstractApi>::Response>
	where
		T::BlockData: Arbitrary,
	{
		match index.election_type {
			EngineElectionType::ByHash(_) => (any::<T::BlockData>(), Just(None)).boxed(),
			EngineElectionType::BlockHeight { submit_hash: false } =>
				(any::<T::BlockData>(), Just(None)).boxed(),
			EngineElectionType::BlockHeight { submit_hash: true } =>
				(any::<T::BlockData>(), any::<ChainBlockHashOf<T::Chain>>().prop_map(Some)).boxed(),
		}
	}

	impl<N: ChainBlockNumberTrait, H: ChainBlockHashTrait, D: BlockDataTrait> BWTypes
		for TypesFor<(N, H, Vec<D>)>
	{
		type ElectionProperties = ();
		type ElectionPropertiesHook = MockHook<HookTypeFor<Self, ElectionPropertiesHook>>;
		type SafeModeEnabledHook = MockHook<HookTypeFor<Self, SafeModeEnabledHook>>;
		type ElectionTrackerDebugEventHook =
			MockHook<HookTypeFor<Self, ElectionTrackerDebugEventHook>>;
	}

	#[test]
	pub fn test_bw_statemachine() {
		type Types1 = TypesFor<(u32, Vec<u8>, Vec<u8>)>;
		BWStatemachine::<Types1>::test(
			file!(),
			generate_state(),
			generate_settings(),
			generate_input::<Types1>,
			generate_context::<Types1>,
		);

		// type Types2 = TypesFor<(u8, Vec<u8>, Vec<u8>)>;
		// BWStatemachine::<Types2>::test(
		// 	file!(),
		// 	generate_state(),
		// 	generate_settings(),
		// 	generate_input::<Types2>,
		// 	generate_context::<Types2>,
		// );
	}
}
