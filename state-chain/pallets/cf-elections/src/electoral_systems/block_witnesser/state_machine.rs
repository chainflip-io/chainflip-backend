use super::{
	super::state_machine::core::*,
	block_processor::{BPChainProgress, BlockProcessorEvent},
	primitives::{ElectionTracker, ElectionTrackerEvent, SafeModeStatus},
};
use crate::electoral_systems::{
	block_height_tracking::{
		ChainBlockHashOf, ChainBlockNumberOf, ChainProgress, ChainTypes, CommonTraits,
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
	type ElectionProperties: Debug + Clone + Encode + Decode + Eq + 'static;
	type ElectionPropertiesHook: Hook<HookTypeFor<Self, ElectionPropertiesHook>> + CommonTraits;
	type SafeModeEnabledHook: Hook<HookTypeFor<Self, SafeModeEnabledHook>> + CommonTraits;

	type ElectionTrackerEventHook: Hook<HookTypeFor<Self, ElectionTrackerEventHook>>
		+ CommonTraits
		+ Default;
}

// hook types
pub struct ElectionTrackerEventHook;
impl<T: BWTypes> HookType for HookTypeFor<T, ElectionTrackerEventHook> {
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
	type Input = (ChainBlockNumberOf<T::Chain>, Range<u32>, T::BlockData, u32);
	type Output = Vec<(ChainBlockNumberOf<T::Chain>, T::Event)>;
}

pub struct ExecuteHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, ExecuteHook> {
	type Input = Vec<(ChainBlockNumberOf<T::Chain>, T::Event)>;
	type Output = ();
}

pub struct LogEventHook;
impl<T: BWProcessorTypes> HookType for HookTypeFor<T, LogEventHook> {
	type Input = BlockProcessorEvent<T>;
	type Output = ();
}

pub trait BWProcessorTypes: Sized + Debug + Clone + Eq {
	type Chain: ChainTypes;
	type BlockData: CommonTraits + Ord + 'static;

	type Event: CommonTraits + Ord + Encode;
	type Rules: Hook<HookTypeFor<Self, RulesHook>> + Default + CommonTraits;
	type Execute: Hook<HookTypeFor<Self, ExecuteHook>> + Default + CommonTraits;

	type LogEventHook: Hook<HookTypeFor<Self, LogEventHook>> + Default + CommonTraits;
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

def_derive! {
	#[no_serde]
	pub struct BWElectionProperties<T: BWTypes> {
		pub election_type: BWElectionType<T::Chain>,
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
	pub enum BWElectionType[C: ChainTypes] {
		/// Querying blocks we haven't received a hash for yet
		Optimistic,

		/// Querying blocks by hash
		ByHash(C::ChainBlockHash),

		/// Querying "old" blocks that are below the safety margin,
		/// and thus we don't care about the hash anymore
		SafeBlockHeight,
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
		use BWElectionType::*;
		// ensure that a hash is only provided for `Optimistic` elections.
		match (&index.election_type, hash) {
			(ByHash(_), None) | (Optimistic, Some(_)) | (SafeBlockHeight, None) => Ok(()),
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
			.map(|(block_height, election_type)| BWElectionProperties {
				properties: state.generate_election_properties_hook.run(block_height),
				election_type,
				block_height,
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
				let removed_block_heights = progress.removed.clone();

				for (height, block) in state.elections.schedule_range(progress) {
					state.block_processor.process_block_data((
						height,
						block.data,
						settings.safety_margin,
					));
				}

				state.block_processor.process_chain_progress(
					BPChainProgress {
						highest_block_height: state
							.elections
							.seen_heights_below
							.saturating_backward(1),
						removed_block_heights,
					},
					// NOTE: we use the lowest "in progress" height for expiring block and event
					// data, this way if one BW election is stuck (e.g. due to failing rpc
					// call), no event data is going to be deleted. That's why we can't use
					// the `highest_seen` block height, because that one progresses always
					// following data from the BHW, ignoring ongoing elections.
					state.elections.lowest_in_progress_height(),
				);
			},

			Either::Left(None) => {},

			Either::Right((properties, (blockdata, blockhash))) => {
				log::info!("got {:?} block data: {:?}", properties.election_type, blockdata);

				if let Some(blockdata) = state.elections.mark_election_done(
					properties.block_height,
					&properties.election_type,
					&blockhash,
					blockdata,
				) {
					state.block_processor.process_block_data_and_chain_progress(
						BPChainProgress {
							highest_block_height: state
								.elections
								.seen_heights_below
								.saturating_backward(1),
							removed_block_heights: None,
						},
						(properties.block_height, blockdata, settings.safety_margin),
						// NOTE: we use the lowest "in progress" height for expiring block and
						// event data, this way if one BW election is stuck (e.g. due to
						// failing rpc call), no event data is going to be deleted. That's
						// why we can't use the `highest_seen` block height, because that one
						// progresses always following data from the BHW, ignoring ongoing
						// elections.
						state.elections.lowest_in_progress_height(),
					)
				}
			},
		};

		state.elections.start_more_elections(
			settings.max_concurrent_elections as usize,
			state.safemode_enabled.run(()),
		);

		Ok(())
	}

	/*
	/// Specifiation for step function
	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &Self::Input,
		_output: &Self::Output,
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
			let scheduled = |s: &Self::State| ChainBlockNumberOf<T::Chain>::steps_between(&s.elections.next_election,&s.elections.next_priority_election).0 ;
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
	*/
}

#[cfg(test)]
pub mod tests {
	// use std::collections::BTreeMap;

	// use proptest::{
	// 	prelude::{any, Arbitrary, Strategy},
	// 	strategy::LazyJust,
	// };

	use super::*;
	use crate::electoral_systems::block_height_tracking::{
		ChainBlockHashTrait, ChainBlockNumberTrait,
	};
	use hook_test_utils::*;

	// const SAFETY_MARGIN: u32 = 3;
	// fn generate_state<
	// 	T: BWTypes<SafeModeEnabledHook = MockHook<HookTypeFor<T, SafeModeEnabledHook>>>,
	// >() -> impl Strategy<Value = BlockWitnesserState<T>>
	// where
	// 	ChainBlockNumberOf<T::Chain>: Arbitrary,
	// 	ChainBlockHashOf<T::Chain>: Arbitrary,
	// 	T::ElectionPropertiesHook: Default + Clone + Debug + Eq,
	// 	T::ElectionTrackerEventHook: Default + Clone + Debug + Eq,
	// 	T::BlockData: Default + Clone + Debug + Eq,
	// {
	// 	prop_do! {
	// 		let (next_election, seen_heights_below,
	// 			 priority_elections_below,
	// 			safemode_enabled) in (
	// 			any::<ChainBlockNumberOf<T::Chain>>(),
	// 			any::<ChainBlockNumberOf<T::Chain>>(),
	// 			any::<ChainBlockNumberOf<T::Chain>>(),
	// 			any::<bool>().prop_map(|b| if b {SafeModeStatus::Enabled} else {SafeModeStatus::Disabled})
	// 		);

	// 		let (ongoing, queued_elections) in
	// 		(
	// 			proptest::collection::vec((any::<ChainBlockNumberOf<T::Chain>>(),
	// any::<BWElectionType<T::Chain>>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move
	// |(height, _)| *height < next_election)),
	// 			proptest::collection::vec((any::<ChainBlockNumberOf<T::Chain>>(),
	// any::<ChainBlockHashOf<T::Chain>>()), 0..10).prop_map(move |xs| xs.into_iter().filter(move
	// |(height, _)| *height < next_election)) 		);
	// 		LazyJust::new(move || BlockWitnesserState {
	// 			elections: ElectionTracker {
	// 				// queued_next_safe_height: None,
	// 				queued_elections: BTreeMap::from_iter(queued_elections.clone()),
	// 				seen_heights_below,
	// 				priority_elections_below,
	// 				ongoing: BTreeMap::from_iter(ongoing.clone()),
	// 				queued_safe_elections: Default::default(),
	// 				optimistic_block_cache: Default::default(),
	// 				debug_events: Default::default()
	// 			},
	// 			generate_election_properties_hook: Default::default(),
	// 			safemode_enabled: MockHook::new(safemode_enabled),
	// 			block_processor: BlockProcessor {
	// 				blocks_data:Default::default(),
	// 				processed_events:Default::default(),
	// 				rules:Default::default(),
	// 				execute:Default::default(),
	// 				debug_events: Default::default()
	// 			},
	// 		})
	// 	}
	// }

	// fn generate_input<T: BWTypes>(
	// 	indices: Vec<Self::Query>,
	// ) -> BoxedStrategy<<BWStatemachine<T> as Statemachine>::Input>
	// where
	// 	ChainBlockNumberOf<T::Chain>: Arbitrary,
	// 	ChainBlockHashOf<T::Chain>: Arbitrary,
	// 	T::BlockData: Arbitrary,
	// {
	// 	let generate_input = |index: BWElectionProperties<T>| {
	// 		prop_oneof![
	// 			(any::<T::BlockData>(), any::<Option<ChainBlockHashOf<T::Chain>>>())
	// 				.prop_map(move |data| (SMInput::Consensus((index.clone(), data)))),
	// 			prop_oneof![
	// 				Just(ChainProgress::None),
	// 				(
	// 					any::<ChainBlockNumberOf<T::Chain>>(),
	// 					btree_map(any::<ChainBlockNumberOf<T::Chain>>(), any::<ChainBlockHashOf<T::Chain>>(),
	// 0..20) 				)
	// 					.prop_map(|(a, hashes)| ChainProgress::Range(
	// 						hashes.clone(),
	// 						a..=a.saturating_forward(hashes.len())
	// 					)),
	// 			]
	// 			.prop_map(SMInput::Context)
	// 		]
	// 	};

	// 	if indices.len() > 0 {
	// 		prop_do! {
	// 			let index in select(indices);
	// 			generate_input(index.clone())
	// 		}
	// 		.boxed()
	// 	} else {
	// 		Just(SMInput::Context(ChainProgress::None)).boxed()
	// 	}
	// }

	impl<
			N: ChainBlockNumberTrait,
			H: ChainBlockHashTrait,
			D: Validate + Ord + Default + CommonTraits + 'static,
		> BWTypes for TypesFor<(N, H, Vec<D>)>
	{
		type ElectionProperties = ();
		type ElectionPropertiesHook = MockHook<HookTypeFor<Self, ElectionPropertiesHook>>;
		type SafeModeEnabledHook = MockHook<HookTypeFor<Self, SafeModeEnabledHook>>;
		type ElectionTrackerEventHook = MockHook<HookTypeFor<Self, ElectionTrackerEventHook>>;
	}

	// type Types = (u32, Vec<u8>, Vec<u8>);

	// #[test]
	// pub fn test_bw_statemachine() {
	// 	BWStatemachine::<Types>::test(
	// 		file!(),
	// 		generate_state(),
	// 		prop_do! {
	// 			let max_concurrent_elections in 0..10u16;
	// 			return BlockWitnesserSettings { max_concurrent_elections, safety_margin: SAFETY_MARGIN}
	// 		},
	// 		generate_input::<Types>,
	// 	);
	// }

	/*

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
	*/
}
