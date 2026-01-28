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

	type Chain: ChainTypes;
	type BlockEntry: BlockDataTrait;
	type ElectionProperties: MaybeArbitrary + CommonTraits;

	type ExecuteHook: Hook<((BlockWitnesserEvent<Self::BlockEntry>, ChainBlockNumberOf<Self::Chain>), ())>
		+ Default
		+ CommonTraits;
	type RulesHook: Hook<((Range<u32>, Vec<Self::BlockEntry>, u32), Vec<BlockWitnesserEvent<Self::BlockEntry>>)>
		+ Default
		+ CommonTraits;

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties;
	fn is_enabled() -> bool;
	fn processed_up_to(height: ChainBlockNumberOf<Self::Chain>);
}

defx! {
	/// Struct that carries all the data associated to a block witnesser. All implementation details
	/// are derived from the given BlockWitnesser instance.
	#[derive(TypeInfo, DefaultNoBound)]
	pub struct DerivedBlockWitnesser[Instance: BlockWitnesserInstance] {
		pub rules: Instance::RulesHook,
		pub execute: Instance::ExecuteHook,
		pub _phantom: sp_std::marker::PhantomData<Instance>,
	}

	validate _this (else DerivedBlockWitnesserError) {}
}

// All the implementations, derived from `I: BlockWitnesserInstance`.
impls! {
	for DerivedBlockWitnesser<I> where (I: BlockWitnesserInstance):

	impl Hook<HookTypeFor<Self, ExecuteHook>> {
		fn run(&mut self, all_events: Vec<(ChainBlockNumberOf<I::Chain>, BlockWitnesserEvent<I::BlockEntry>)>) {

			// ------ deduplicate events -------
			let mut chosen_events: BTreeMap<I::BlockEntry, _> = BTreeMap::new();

			for (block_height, event) in all_events {
				let witness = event.inner_witness().clone();

				// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
				if !chosen_events.contains_key(&witness) ||
					(matches!(chosen_events.get(&witness), Some((_, BlockWitnesserEvent::PreWitness(_)))) &&
						matches!(event, BlockWitnesserEvent::Witness(_)))
				{
					chosen_events.insert(witness, (block_height, event));
				}
			}

			// ------ execute events -------
			for (block_height, event) in chosen_events.into_values() {
				self.execute.run((event, block_height));
			}
		}
	}

	impl Hook<HookTypeFor<Self, RulesHook>> {
		fn run(&mut self, input: (Range<u32>, Vec<I::BlockEntry>, u32)) -> Vec<BlockWitnesserEvent<I::BlockEntry>> {
			self.rules.run(input)
		}
	}

	impl Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if I::is_enabled() {
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	impl BWProcessorTypes {
		type Chain = I::Chain;
		type BlockData = Vec<I::BlockEntry>;
		type Event = BlockWitnesserEvent<I::BlockEntry>;
		type Rules = Self;
		type Execute = Self;
		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = I::BWNAME;
	}

	impl BWTypes {
		type ElectionProperties = I::ElectionProperties;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	impl StatemachineElectoralSystemTypes {
		type ValidatorId = <I::Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<I::Runtime>;
		type OnFinalizeReturnItem = ();
		type VoteStorage =
			vote_storage::bitmap::Bitmap<(Vec<I::BlockEntry>, Option<ChainBlockHashOf<I::Chain>>)>;
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
		type ElectoralSettings = ();
	}

	impl Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) -> I::ElectionProperties {
			I::election_properties(input)
		}
	}

	impl Hook<HookTypeFor<Self, ProcessedUpToHook>> {
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
