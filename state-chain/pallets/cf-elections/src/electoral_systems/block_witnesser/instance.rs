use cf_traits::Chainflip;
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
		state_machine::{
			core::{defx, hook_test_utils::EmptyHook, Hook, Validate},
			state_machine_es::StatemachineElectoralSystemTypes,
		},
	},
	generic_tools::*,
	vote_storage,
};

use sp_std::vec::Vec;

pub trait BlockWitnesserInstance: CommonTraits + Validate + Member {
	const BWNAME: &'static str;

	type Config: Chainflip;

	// TODO move these conditions as deep as possible (into the "common" trait definitions)
	type Chain: ChainTypes<
		ChainBlockHash: Parameter + Send + Sync,
		ChainBlockNumber: Parameter + Send + Sync,
	>;
	type BlockData: BlockDataTrait + Parameter + Send + Sync;
	type Event: CommonTraits + Ord + Encode + Member;
	type ElectionProperties: MaybeArbitrary + CommonTraits + TestTraits + Send + Sync;

	fn execute(events: Vec<(ChainBlockNumberOf<Self::Chain>, Self::Event)>);
	fn rules(block: (Range<u32>, Self::BlockData, u32)) -> Vec<Self::Event>;
	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties;
	fn is_enabled() -> bool;
	fn processed_up_to(height: ChainBlockNumberOf<Self::Chain>);
}

defx! {
	#[derive(TypeInfo, DefaultNoBound)]
	pub struct DerivedBlockWitnesser[Instance: BlockWitnesserInstance] {
		pub _phantom: sp_std::marker::PhantomData<Instance>,
	}

	validate _this (else DerivedBlockWitnesserError) {}
}

impl<I: BlockWitnesserInstance> Hook<HookTypeFor<DerivedBlockWitnesser<I>, ExecuteHook>>
	for DerivedBlockWitnesser<I>
{
	fn run(&mut self, input: Vec<(ChainBlockNumberOf<I::Chain>, I::Event)>) {
		I::execute(input);
	}
}

impl<I: BlockWitnesserInstance> Hook<HookTypeFor<DerivedBlockWitnesser<I>, RulesHook>>
	for DerivedBlockWitnesser<I>
{
	fn run(&mut self, input: (Range<u32>, I::BlockData, u32)) -> Vec<I::Event> {
		I::rules(input)
	}
}

impl<I: BlockWitnesserInstance> BWProcessorTypes for DerivedBlockWitnesser<I> {
	type Chain = I::Chain;
	type BlockData = I::BlockData;
	type Event = I::Event;
	type Rules = Self;
	type Execute = Self;
	type DebugEventHook = EmptyHook;

	const BWNAME: &'static str = I::BWNAME;
}

impl<I: BlockWitnesserInstance> Hook<HookTypeFor<DerivedBlockWitnesser<I>, ElectionPropertiesHook>>
	for DerivedBlockWitnesser<I>
{
	fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) -> I::ElectionProperties {
		I::election_properties(input)
	}
}

impl<I: BlockWitnesserInstance> Hook<HookTypeFor<DerivedBlockWitnesser<I>, SafeModeEnabledHook>>
	for DerivedBlockWitnesser<I>
{
	fn run(&mut self, _input: ()) -> SafeModeStatus {
		if I::is_enabled() {
			SafeModeStatus::Disabled
		} else {
			SafeModeStatus::Enabled
		}
	}
}

impl<I: BlockWitnesserInstance> Hook<HookTypeFor<DerivedBlockWitnesser<I>, ProcessedUpToHook>>
	for DerivedBlockWitnesser<I>
{
	fn run(&mut self, input: ChainBlockNumberOf<I::Chain>) {
		I::processed_up_to(input);
	}
}

impl<I: BlockWitnesserInstance> BWTypes for DerivedBlockWitnesser<I> {
	type ElectionProperties = I::ElectionProperties;
	type ElectionPropertiesHook = Self;
	type SafeModeEnabledHook = Self;
	type ProcessedUpToHook = Self;
	type ElectionTrackerDebugEventHook = EmptyHook;
}

impl<I: BlockWitnesserInstance> StatemachineElectoralSystemTypes for DerivedBlockWitnesser<I> {
	type ValidatorId = <I::Config as Chainflip>::ValidatorId;
	type StateChainBlockNumber = BlockNumberFor<I::Config>;
	type OnFinalizeReturnItem = ();
	type VoteStorage =
		vote_storage::bitmap::Bitmap<(I::BlockData, Option<ChainBlockHashOf<I::Chain>>)>;
	type Statemachine = BWStatemachine<Self>;
	type ConsensusMechanism = BWConsensus<Self>;
	type ElectoralSettings = ();
}
