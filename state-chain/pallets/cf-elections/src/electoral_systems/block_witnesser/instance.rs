use cf_chains::{witness_period::BlockWitnessRange, ChainWitnessConfig};
use cf_primitives::BlockWitnesserEvent;
use cf_traits::Chainflip;
use cf_utilities::impls;
use core::ops::Range;
use frame_support::{pallet_prelude::*, DefaultNoBound};
use sp_std::collections::btree_map::BTreeMap;

use frame_system::pallet_prelude::BlockNumberFor;
use serde::{Deserialize, Serialize};

use crate::{
	electoral_systems::{
		block_height_witnesser::{ChainBlockHashOf, ChainBlockNumberOf, ChainTypes},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{
				BWProcessorTypes, BWStatemachine, BWTypes, BlockDataTrait, ElectionPropertiesHook,
				ExecuteHook, HookTypeFor, ProcessedUpToHook, RulesHook, SafeModeEnabledHook,
			},
		},
		state_machine::{core::defx, state_machine_es::StatemachineElectoralSystemTypes},
	},
	generic_tools::*,
	vote_storage,
};
use cf_traits::{hook_test_utils::EmptyHook, Hook, Validate};
use sp_std::vec::Vec;

/// A new BlockWitnesser electoral system instance can be derived by implementing this trait.
/// Once you have `I: BlockWitnesserInstance` then `DerivedBlockWitnesser<I>` implements all
/// the traits that are required for a block witnesser.
pub trait BlockWitnesserInstance: CommonTraits + Validate + Member {
	const BWNAME: &'static str;

	type Runtime: Chainflip;

	type Chain: ChainTypes<ChainBlockNumber: HasWitnessRoot>;
	type BlockEntry: BlockDataTrait;
	type ElectionProperties: MaybeArbitrary + CommonTraits;

	type ExecutionTarget: Hook<(
			(
				BlockWitnesserEvent<Self::BlockEntry>,
				<ChainBlockNumberOf<Self::Chain> as HasWitnessRoot>::Root,
			),
			(),
		)> + Default
		+ CommonTraits;
	type WitnessRules: Hook<((Range<u32>, Vec<Self::BlockEntry>, u32), Vec<BlockWitnesserEvent<Self::BlockEntry>>)>
		+ Default
		+ CommonTraits;

	fn is_enabled() -> bool;
	fn election_properties(
		block_height: ChainBlockNumberOf<Self::Chain>,
	) -> Self::ElectionProperties;
	fn processed_up_to(block_height: ChainBlockNumberOf<Self::Chain>);
}

defx! {
	/// Struct that carries all the data associated to a block witnesser. All implementation details
	/// are derived from the given BlockWitnesser instance.
	#[derive(TypeInfo, DefaultNoBound)]
	pub struct GenericBlockWitnesser[Instance: BlockWitnesserInstance] {
		pub rules: Instance::WitnessRules,
		pub execute: Instance::ExecutionTarget,
		pub _phantom: sp_std::marker::PhantomData<Instance>,
	}

	validate _this (else GenericBlockWitnesserError) {}
}

// All the implementations, derived from `I: BlockWitnesserInstance`.
impls! {
	for GenericBlockWitnesser<I> where (I: BlockWitnesserInstance):

	Hook<HookTypeFor<Self, ExecuteHook>> {
		fn run(&mut self, all_events: Vec<(ChainBlockNumberOf<I::Chain>, BlockWitnesserEvent<I::BlockEntry>)>) {
			for (block_height, event) in dedup_events(all_events) {
				self.execute.run((event, block_height.get_root()));
			}
		}
	}

	Hook<HookTypeFor<Self, RulesHook>> {
		fn run(&mut self, input: (Range<u32>, Vec<I::BlockEntry>, u32)) -> Vec<BlockWitnesserEvent<I::BlockEntry>> {
			self.rules.run(input)
		}
	}

	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if I::is_enabled() {
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	BWProcessorTypes {
		type Chain = I::Chain;
		type BlockData = Vec<I::BlockEntry>;
		type Event = BlockWitnesserEvent<I::BlockEntry>;
		type Rules = Self;
		type Execute = Self;
		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = I::BWNAME;
	}

	BWTypes {
		type ElectionProperties = I::ElectionProperties;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	StatemachineElectoralSystemTypes {
		type ValidatorId = <I::Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<I::Runtime>;
		type OnFinalizeReturnItem = ();
		type VoteStorage =
			vote_storage::bitmap::Bitmap<(Vec<I::BlockEntry>, Option<ChainBlockHashOf<I::Chain>>)>;
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
		type ElectoralSettings = ();
	}

	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) -> I::ElectionProperties {
			I::election_properties(input)
		}
	}

	Hook<HookTypeFor<Self, ProcessedUpToHook>> {
		fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) {
			I::processed_up_to(input);
		}
	}
}

// ------------------ rules hook implementations -----------------
// We currently have two different rulesets for dispatching BW events:
// 1. Just witness when it has reached the safety margin
// 2. Prewitness when a deposit has age 0, and witness when it reached the safety margin
//
// The (1.) method is used by Arbitrum and Ethereum, and (2.) is used by Bitcoin witnessing.
//
// Here both rulesets are defined generically, such that BW instantiations just have to
// reference either of:
// - JustWitnessAtSafetyMargin
// - PrewitnessImmediatelyAndWitnessAtSafetyMargin

define_empty_struct! {
	pub struct JustWitnessAtSafetyMargin<BlockEntry>;
}

impl<BlockEntry> Hook<((Range<u32>, Vec<BlockEntry>, u32), Vec<BlockWitnesserEvent<BlockEntry>>)>
	for JustWitnessAtSafetyMargin<BlockEntry>
{
	fn run(&mut self, (ages, block_data, safety_margin): <((Range<u32>, Vec<BlockEntry>, u32), Vec<BlockWitnesserEvent<BlockEntry>>) as cf_traits::HookType>::Input) -> <((Range<u32>, BlockEntry, u32), Vec<BlockWitnesserEvent<BlockEntry>>) as cf_traits::HookType>::Output{
		if ages.contains(&safety_margin) {
			block_data.into_iter().map(BlockWitnesserEvent::Witness).collect()
		} else {
			Vec::new()
		}
	}
}

define_empty_struct! {
	pub struct PrewitnessImmediatelyAndWitnessAtSafetyMargin<BlockEntry>;
}

impl<BlockEntry: Clone>
	Hook<((Range<u32>, Vec<BlockEntry>, u32), Vec<BlockWitnesserEvent<BlockEntry>>)>
	for PrewitnessImmediatelyAndWitnessAtSafetyMargin<BlockEntry>
{
	fn run(&mut self, (ages, block_data, safety_margin): <((Range<u32>, Vec<BlockEntry>, u32), Vec<BlockWitnesserEvent<BlockEntry>>) as cf_traits::HookType>::Input) -> <((Range<u32>, Vec<BlockEntry>, u32), Vec<BlockWitnesserEvent<BlockEntry>>) as cf_traits::HookType>::Output{
		let mut results: Vec<BlockWitnesserEvent<BlockEntry>> = Vec::new();
		if ages.contains(&0u32) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| BlockWitnesserEvent::PreWitness(vault_deposit.clone()))
					.collect::<Vec<_>>(),
			)
		}
		if ages.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| BlockWitnesserEvent::Witness(vault_deposit.clone()))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

// ------------------ helper implementations -----------------

fn dedup_events<BlockNumber, T: Ord + Clone>(
	events: Vec<(BlockNumber, BlockWitnesserEvent<T>)>,
) -> Vec<(BlockNumber, BlockWitnesserEvent<T>)> {
	let mut chosen: BTreeMap<T, (BlockNumber, BlockWitnesserEvent<T>)> = BTreeMap::new();

	for (block, event) in events {
		let witness = event.inner_witness().clone();

		// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
		if !chosen.contains_key(&witness) ||
			(matches!(chosen.get(&witness), Some((_, BlockWitnesserEvent::PreWitness(_)))) &&
				matches!(event, BlockWitnesserEvent::Witness(_)))
		{
			chosen.insert(witness, (block, event));
		}
	}

	chosen.into_values().collect()
}

#[test]
fn dedup_events_test() {
	use cf_primitives::BlockWitnesserEvent;
	let events = vec![
		(10, BlockWitnesserEvent::<u8>::Witness(9)),
		(8, BlockWitnesserEvent::<u8>::PreWitness(9)),
		(10, BlockWitnesserEvent::<u8>::Witness(10)),
		(10, BlockWitnesserEvent::<u8>::Witness(11)),
		(8, BlockWitnesserEvent::<u8>::PreWitness(11)),
		(10, BlockWitnesserEvent::<u8>::PreWitness(12)),
	];
	let deduped_events = dedup_events(events);

	assert_eq!(
		deduped_events,
		vec![
			(10, BlockWitnesserEvent::<u8>::Witness(9)),
			(10, BlockWitnesserEvent::<u8>::Witness(10)),
			(10, BlockWitnesserEvent::<u8>::Witness(11)),
			(10, BlockWitnesserEvent::<u8>::PreWitness(12)),
		]
	)
}

/// This trait is only temporary. It's required because currently in the `Chain` trait
/// implementation the ChainBlockNumber type is u64, but in the arbitrum elections the block number
/// type is BlockWitnessRange<>. As long as the old witnessing code exists, it is difficult to
/// change the ChainBlockNumber type of the Chain trait, because code relies on the fact that it's a
/// number.
///
/// When assethub is removed, the ChainBlockNumber type of Arbitrum should be changed to
/// BlockWitnessRange and this trait can then be removed.
pub trait HasWitnessRoot {
	type Root;
	fn get_root(&self) -> Self::Root;
}

impl<C: ChainWitnessConfig> HasWitnessRoot for BlockWitnessRange<C> {
	type Root = C::ChainBlockNumber;

	fn get_root(&self) -> Self::Root {
		*self.root()
	}
}

impl HasWitnessRoot for u64 {
	type Root = u64;

	fn get_root(&self) -> Self::Root {
		*self
	}
}

/// This trait is only temporary. It's required because currently in the `Chain` trait
/// implementation the ChainBlockNumber type is u64, but in the ethereum elections the block number
/// type is BlockWitnessRange<>. As long as the old witnessing code exists, it is difficult to
/// change the ChainBlockNumber type of the Chain trait, because code relies on the fact that it's a
/// number.
///
/// When assethub is removed, the ChainBlockNumber type of Arbitrum should be changed to
/// BlockWitnessRange and this trait can then be removed.
pub trait HasWitnessRoot {
	type Root;
	fn get_root(&self) -> Self::Root;
}

impl<C: ChainWitnessConfig> HasWitnessRoot for BlockWitnessRange<C> {
	type Root = C::ChainBlockNumber;

	fn get_root(&self) -> Self::Root {
		*self.root()
	}
}

impl HasWitnessRoot for u64 {
	type Root = u64;

	fn get_root(&self) -> Self::Root {
		*self
	}
}
