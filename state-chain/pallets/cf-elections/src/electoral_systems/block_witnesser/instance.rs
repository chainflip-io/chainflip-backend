use cf_traits::Chainflip;
use cf_utilities::impls;
use core::ops::Range;
use frame_support::{pallet_prelude::*, DefaultNoBound};

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

pub trait BlockWitnesserInstance: CommonTraits + Validate + Member {
	const BWNAME: &'static str;

	type Runtime: Chainflip;

	type Chain: ChainTypes;
	type BlockEntry: BlockDataTrait;
	type Event: CommonTraits + Ord;
	type ElectionProperties: MaybeArbitrary + CommonTraits;

	type ExecuteHook: Hook<((Self::Event, ChainBlockNumberOf<Self::Chain>), ())>
		+ Default
		+ CommonTraits;
	type RulesHook: Hook<((Range<u32>, Vec<Self::BlockEntry>, u32), Vec<Self::Event>)>
		+ Default
		+ CommonTraits;

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties;
	fn is_enabled() -> bool;
	fn processed_up_to(height: ChainBlockNumberOf<Self::Chain>);
}

defx! {
	#[derive(TypeInfo, DefaultNoBound)]
	pub struct DerivedBlockWitnesser[Instance: BlockWitnesserInstance] {
		pub rules: Instance::RulesHook,
		pub execute: Instance::ExecuteHook,
		pub _phantom: sp_std::marker::PhantomData<Instance>,
	}

	validate _this (else DerivedBlockWitnesserError) {}
}

impls! {
	for DerivedBlockWitnesser<I> where (I: BlockWitnesserInstance):

	impl Hook<HookTypeFor<Self, ExecuteHook>> {
		fn run(&mut self, input: Vec<(ChainBlockNumberOf<I::Chain>, I::Event)>) {
			// TODO: deduplicate!
			for (block_height, event) in input {
				self.execute.run((event, block_height));
			}
		}
	}

	impl Hook<HookTypeFor<Self, RulesHook>> {
		fn run(&mut self, input: (Range<u32>, Vec<I::BlockEntry>, u32)) -> Vec<I::Event> {
			self.rules.run(input)
		}
	}

	impl Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) -> I::ElectionProperties {
			I::election_properties(input)
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

	impl Hook<HookTypeFor<Self, ProcessedUpToHook>> {
		fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) {
			I::processed_up_to(input);
		}
	}

	impl BWProcessorTypes {
		type Chain = I::Chain;
		type BlockData = Vec<I::BlockEntry>;
		type Event = I::Event;
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

}
