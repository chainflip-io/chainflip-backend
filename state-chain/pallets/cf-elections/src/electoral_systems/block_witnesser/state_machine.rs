use super::{
	super::state_machine::core::*,
	block_processor::BlockProcessorEvent,
	primitives::{ElectionTracker, ElectionTrackerEvent, SafeModeStatus},
};
#[cfg(test)]
use crate::electoral_systems::state_machine::state_machine::InputOf;
use crate::electoral_systems::{
	block_height_witnesser::{
		ChainBlockHashOf, ChainBlockNumberOf, ChainProgress, ChainTypes, CommonTraits,
		MaybeArbitrary, TestTraits,
	},
	block_witnesser::block_processor::BlockProcessor,
	state_machine::{
		core::Validate,
		state_machine::{AbstractApi, Statemachine},
	},
};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use core::ops::Range;
use derive_where::derive_where;
use generic_typeinfo_derive::GenericTypeInfo;
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
	type ProcessedUpToHook: Hook<HookTypeFor<Self, ProcessedUpToHook>> + CommonTraits;

	type ElectionTrackerDebugEventHook: Hook<HookTypeFor<Self, ElectionTrackerDebugEventHook>>
		+ CommonTraits
		+ TestTraits
		+ Default;

	const BWNAME: &'static str;
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

pub struct ProcessedUpToHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, ProcessedUpToHook> {
	type Input = ChainBlockNumberOf<T::Chain>;
	type Output = ();
}

pub struct DebugEventHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, DebugEventHook> {
	type Input = BlockProcessorEvent<T>;
	type Output = ();
}

pub trait BlockDataTrait = CommonTraits + TestTraits + MaybeArbitrary + Ord + 'static;
pub trait BWProcessorTypes: Sized + 'static + Debug + Clone + Eq {
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
	pub max_optimistic_elections: u8,
	pub safety_margin: u32,
}

defx! {
	#[derive(GenericTypeInfo)]
	#[expand_name_with(T::Chain::NAME)]
	#[derive(Default)]
	pub struct BlockWitnesserState[T: BWTypes] {
		pub elections: ElectionTracker<T>,
		pub generate_election_properties_hook: T::ElectionPropertiesHook,
		pub safemode_enabled: T::SafeModeEnabledHook,
		pub block_processor: BlockProcessor<T>,
		pub processed_up_to: T::ProcessedUpToHook,
	}
	validate _this (else BlockWitnesserError) {}
}

def_derive!(
	#[derive(GenericTypeInfo)]
	#[expand_name_with(C::NAME)]
	pub enum EngineElectionType<C: ChainTypes> {
		ByHash(C::ChainBlockHash),
		BlockHeight { submit_hash: bool },
	}
);
def_derive! {
	#[no_serde]
	#[derive(GenericTypeInfo)]
	#[expand_name_with(scale_info::prelude::format!("{}{}", T::Chain::NAME, T::BWNAME))]
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
	#[derive(GenericTypeInfo)]
	#[expand_name_with(scale_info::prelude::format!("{}{}", T::Chain::NAME, T::BWNAME))]
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

	fn get_queries(state: &mut Self::State) -> Vec<Self::Query> {
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

		let lowest_in_progress_height = state.elections.lowest_in_progress_height();

		state.block_processor.process_blocks_up_to(
			state.elections.seen_heights_below,
			// NOTE: we use the lowest "in progress" height for expiring block and event
			// data, this way if one BW election is stuck (e.g. due to failing rpc
			// call), no event data is going to be deleted. That's why we can't use
			// the `highest_seen` block height, because that one progresses always
			// following data from the BHW, ignoring ongoing elections.
			lowest_in_progress_height,
		);

		state.elections.start_more_elections(
			settings.max_ongoing_elections as usize,
			settings.max_optimistic_elections,
			state.safemode_enabled.run(()),
		);

		// We subtract 1 since that is the block that we have processed up to.
		state.processed_up_to.run(lowest_in_progress_height.saturating_backward(1));

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

		assert!(
			before.elections.seen_heights_below <= after.elections.seen_heights_below,
			"`seen_heights_below` should be monotonically increasing"
		);

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
		let get_all_heights = |s: &Self::State, remove_expired: bool| {
			s.elections
				.ongoing
				.iter()
				.filter(|(_, t)| **t != BWElectionType::Optimistic)
				.map(|(h, _)| h)
				.cloned()
				.chain(s.elections.queued_hash_elections.keys().cloned())
				.chain(s.elections.queued_safe_elections.get_all_heights())
				.chain(
					s.block_processor
						.blocks_data
						.keys()
						.filter(|h| {
							if remove_expired {
								h.saturating_forward(T::Chain::SAFETY_BUFFER) <
									s.elections.seen_heights_below
							} else {
								true
							}
						})
						.cloned(),
				)
				.collect::<Vec<_>>()
		};

		let counted_heights: Container<BTreeMultiSet<_>> =
			get_all_heights(after, false).into_iter().collect();

		// we have unique heights
		for (height, count) in counted_heights.0 .0.clone() {
			if count > 1 {
				panic!("Got height {height:?} in total {count} times");
			}
		}

		let (new_heights, removed_heights, received_height) = match input {
			Either::Left(Some(progress)) => (
				progress
					.headers
					.headers
					.iter()
					.map(|block| block.block_height)
					.collect::<Vec<_>>(),
				progress.removed.clone(),
				None,
			),
			Either::Left(None) => Default::default(),
			Either::Right((properties, _)) =>
				(Default::default(), Default::default(), Some(properties.block_height)),
		};

		assert_eq!(
			get_all_heights(before, true)
				.into_iter()
				.filter(|h| *h < after.elections.seen_heights_below)
				.filter(|h| !removed_heights.iter().any(|range| range.contains(h)))
				.chain(new_heights)
				.filter(|h| h.saturating_forward(T::Chain::SAFETY_BUFFER) >= after.elections.seen_heights_below || Some(h) != received_height.as_ref())
				.collect::<BTreeSet<_>>(),
			get_all_heights(after, false)
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
	use proptest::{arbitrary::arbitrary_with, prelude::*, prop_oneof};

	use super::*;
	use crate::{
		electoral_systems::block_height_witnesser::{ChainBlockHashTrait, ChainBlockNumberTrait},
		prop_do,
	};
	use hook_test_utils::*;

	fn generate_state<
		T: BWTypes<SafeModeEnabledHook = MockHook<HookTypeFor<T, SafeModeEnabledHook>>>,
	>() -> impl Strategy<Value = BlockWitnesserState<T>>
	where
		T::ElectionPropertiesHook: Default,
		T::ProcessedUpToHook: Default,
		T::BlockData: Default,
	{
		(any::<SafeModeStatus>(), any::<ElectionTracker<T>>()).prop_map(|(safemode, elections)| {
			BlockWitnesserState {
				elections,
				generate_election_properties_hook: Default::default(),
				safemode_enabled: MockHook::new(ConstantHook::new(safemode)),
				processed_up_to: Default::default(),
				block_processor: Default::default(),
			}
		})
	}

	fn generate_context<T: BWTypes>(
		state: &BlockWitnesserState<T>,
	) -> BoxedStrategy<Option<ChainProgress<T::Chain>>>
	where
		T::Chain: Arbitrary,
	{
		let safe_block_height =
			state.elections.seen_heights_below.saturating_backward(T::Chain::SAFETY_BUFFER);

		let seen_heights_below = state.elections.seen_heights_below;

		prop_oneof![
			Just(None),
			(0..T::Chain::SAFETY_BUFFER)
				.prop_flat_map(move |x| arbitrary_with::<ChainProgress<T::Chain>, _, _>((
					safe_block_height.saturating_forward(x),
					Default::default()
				)))
				.prop_map(move |mut progress| {
					if let Some(x) = progress.headers.headers.front() {
						progress.removed =
							Some(x.block_height..=seen_heights_below.saturating_backward(1));
					}

					Some(progress)
				})
		]
		.boxed()
	}

	fn generate_settings() -> impl Strategy<Value = BlockWitnesserSettings> + Clone + Sync + Send {
		prop_do! {
			BlockWitnesserSettings {
				safety_margin: 1..5u32,
				max_ongoing_elections: 1..10u16,
				max_optimistic_elections: 0..3u8,
			}
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
		type ProcessedUpToHook = MockHook<HookTypeFor<Self, ProcessedUpToHook>>;

		const BWNAME: &'static str = "GenericBW";
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
	}
}
