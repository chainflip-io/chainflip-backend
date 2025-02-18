use sp_std::{collections::btree_map::BTreeMap, iter::Step, vec, vec::Vec};

use crate::{chainflip::bitcoin_elections::BlockData, BitcoinIngressEgress, Runtime};
use cf_chains::{btc::BlockNumber, instances::BitcoinInstance};
use cf_primitives::chains::Bitcoin;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};

use log::warn;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::state_machine::BWProcessorTypes, state_machine::core::Hook,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum BtcEvent {
	PreWitness(DepositWitness<Bitcoin>),
	Witness(DepositWitness<Bitcoin>),
}

impl BtcEvent {
	fn deposit_witness(&self) -> &DepositWitness<Bitcoin> {
		match self {
			BtcEvent::PreWitness(dw) | BtcEvent::Witness(dw) => dw,
		}
	}
}

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct ExecuteEventHook {}
impl Hook<(BlockNumber, BtcEvent), ()> for ExecuteEventHook {
	fn run(&mut self, (block, input): (BlockNumber, BtcEvent)) {
		match input {
			BtcEvent::PreWitness(deposit) => {
				let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(deposit, block);
			},
			BtcEvent::Witness(deposit) => {
				BitcoinIngressEgress::process_channel_deposit_full_witness(deposit, block);
				warn!("Witness executed");
				ProcessedUpTo::<Runtime, BitcoinInstance>::set(block);
			},
		}
	}
}
#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct ApplyRulesHook {}
impl Hook<(BlockNumber, u32, BlockData), Vec<(BlockNumber, BtcEvent)>> for ApplyRulesHook {
	fn run(
		&mut self,
		(block, age, block_data): (BlockNumber, u32, BlockData),
	) -> Vec<(BlockNumber, BtcEvent)> {
		// Prewitness rule
		if age == 0 {
			return block_data
				.iter()
				.map(|deposit_witness| (block, BtcEvent::PreWitness(deposit_witness.clone())))
				.collect::<Vec<(BlockNumber, BtcEvent)>>();
		}
		//Full witness rule
		if age ==
			u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0
				as u32
		{
			return block_data
				.iter()
				.map(|deposit_witness| (block, BtcEvent::Witness(deposit_witness.clone())))
				.collect::<Vec<(BlockNumber, BtcEvent)>>();
		}
		vec![]
	}
}

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct DedupEventsHook {}
/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep only the `Witness` variant,
impl Hook<Vec<(BlockNumber, BtcEvent)>, Vec<(BlockNumber, BtcEvent)>> for DedupEventsHook {
	fn run(&mut self, events: Vec<(BlockNumber, BtcEvent)>) -> Vec<(BlockNumber, BtcEvent)> {
		// Map: deposit_witness -> chosen BtcEvent
		// todo! this is annoying, it require us to implement Ord down to the Chain type
		let mut chosen: BTreeMap<DepositWitness<Bitcoin>, (BlockNumber, BtcEvent)> =
			BTreeMap::new();

		for (block, event) in events {
			let deposit: DepositWitness<Bitcoin> = event.deposit_witness().clone();

			match chosen.get(&deposit) {
				None => {
					// No event yet for this deposit, store it
					chosen.insert(deposit, (block, event));
				},
				Some((_, existing_event)) => {
					// There's already an event for this deposit
					match (existing_event, &event) {
						// If we already have a Witness, do nothing
						(BtcEvent::Witness(_), BtcEvent::PreWitness(_)) => (),
						// If we have a PreWitness and the new event is a Witness, override it
						(BtcEvent::PreWitness(_), BtcEvent::Witness(_)) => {
							chosen.insert(deposit, (block, event));
						},
						// This should be impossible to reach!
						(_, _) => (),
					}
				},
			}
		}
		chosen.into_values().collect()
	}
}

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct SafetyMarginHook {}

impl Hook<(), u32> for SafetyMarginHook {
	fn run(&mut self, _input: ()) -> u32 {
		u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0 as u32
	}
}
#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct BlockWitnessingProcessorDefinition {}

impl BWProcessorTypes for BlockWitnessingProcessorDefinition {
	type ChainBlockNumber = BlockNumber;
	type BlockData = BlockData;
	type Event = BtcEvent;
	type Rules = ApplyRulesHook;
	type Execute = ExecuteEventHook;
	type DedupEvents = DedupEventsHook;
	type SafetyMargin = SafetyMarginHook;
}

#[cfg(test)]
mod tests {
	use cf_chains::btc::BlockNumber;
	use std::collections::BTreeMap;

	use crate::chainflip::bitcoin_block_processor::{ApplyRulesHook, SafetyMarginHook};
	use codec::{Decode, Encode};
	use core::ops::RangeInclusive;
	use frame_support::pallet_prelude::TypeInfo;
	use pallet_cf_elections::electoral_systems::block_witnesser::primitives::ChainProgressInner;

	use crate::chainflip::bitcoin_block_processor::DedupEventsHook;
	use cf_chains::btc::BtcAmount;
	use pallet_cf_elections::electoral_systems::{
		block_witnesser::{
			block_processor::{BlockProcessor, SMBlockProcessorInput},
			state_machine::BWProcessorTypes,
		},
		state_machine::core::{hook_test_utils::IncreasingHook, Hook},
	};
	use proptest::{
		prelude::{any, prop, BoxedStrategy, Strategy},
		prop_oneof,
	};
	use serde::{Deserialize, Serialize};

	#[allow(dead_code)]
	fn block_data() -> BoxedStrategy<MockDeposit> {
		(any::<u64>(), any::<u32>())
			.prop_map(|(amount, numb)| MockDeposit { amount, deposit_address: numb.to_string() })
			.boxed()
	}
	#[allow(dead_code)]
	fn blocks_data(
		number_of_blocks: u64,
	) -> BoxedStrategy<BTreeMap<BlockNumber, (MockBlockData, u32)>> {
		prop::collection::btree_map(
			0..number_of_blocks,
			(vec![block_data()], (0..=0u32)),
			RangeInclusive::new(0, number_of_blocks as usize),
		)
		.boxed()
	}
	#[allow(dead_code)]
	fn generate_state() -> BoxedStrategy<BlockProcessor<MockBlockProcessorDefinition>> {
		blocks_data(10)
			.prop_map(|data| BlockProcessor {
				blocks_data: data,
				reorg_events: Default::default(),
				rules: ApplyRulesHook {},
				execute: IncreasingHook::<(BlockNumber, MockBtcEvent), ()>::default(),
				dedup_events: DedupEventsHook {},
				safety_margin: SafetyMarginHook {},
			})
			.boxed()
	}
	#[allow(dead_code)]
	fn generate_input() -> BoxedStrategy<SMBlockProcessorInput<MockBlockProcessorDefinition>> {
		prop_oneof![
			(any::<u64>(), block_data()).prop_map(|(n, data)| SMBlockProcessorInput::NewBlockData(
				n,
				n,
				vec![data]
			)),
			prop_oneof![
				(0..=5u64).prop_map(ChainProgressInner::Progress),
				(0..=5u64).prop_map(|n| ChainProgressInner::Reorg(
					RangeInclusive::<BlockNumber>::new(n, n + 2)
				)),
			]
			.prop_map(SMBlockProcessorInput::ChainProgress),
		]
		.boxed()
	}

	#[derive(
		Clone,
		PartialEq,
		Eq,
		Serialize,
		Deserialize,
		Encode,
		TypeInfo,
		Decode,
		Debug,
		Ord,
		PartialOrd,
		Default,
	)]
	struct MockBlockProcessorDefinition {}
	#[derive(
		Clone,
		PartialEq,
		Eq,
		Serialize,
		Deserialize,
		Encode,
		TypeInfo,
		Decode,
		Debug,
		Ord,
		PartialOrd,
		Default,
	)]
	struct MockDeposit {
		pub amount: BtcAmount,
		pub deposit_address: String,
	}
	type MockBlockData = Vec<MockDeposit>;

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
	enum MockBtcEvent {
		PreWitness(MockDeposit),
		Witness(MockDeposit),
	}
	impl MockBtcEvent {
		pub fn deposit_witness(&self) -> &MockDeposit {
			match self {
				MockBtcEvent::PreWitness(dw) | MockBtcEvent::Witness(dw) => dw,
			}
		}
	}

	impl Hook<(BlockNumber, u32, MockBlockData), Vec<(BlockNumber, MockBtcEvent)>> for ApplyRulesHook {
		fn run(
			&mut self,
			(block, age, block_data): (BlockNumber, u32, MockBlockData),
		) -> Vec<(BlockNumber, MockBtcEvent)> {
			// Prewitness rule
			if age == 0 {
				return block_data
					.iter()
					.map(|deposit_witness| {
						(block, MockBtcEvent::PreWitness(deposit_witness.clone()))
					})
					.collect::<Vec<(BlockNumber, MockBtcEvent)>>();
			}
			//Full witness rule
			if age == 3 {
				return block_data
					.iter()
					.map(|deposit_witness| (block, MockBtcEvent::Witness(deposit_witness.clone())))
					.collect::<Vec<(BlockNumber, MockBtcEvent)>>();
			}
			vec![]
		}
	}

	impl Hook<Vec<(BlockNumber, MockBtcEvent)>, Vec<(BlockNumber, MockBtcEvent)>> for DedupEventsHook {
		fn run(
			&mut self,
			events: Vec<(BlockNumber, MockBtcEvent)>,
		) -> Vec<(BlockNumber, MockBtcEvent)> {
			// Map: deposit_witness -> chosen BtcEvent
			// todo! this is annoying, it require us to implement Ord down to the Chain type
			let mut chosen: BTreeMap<MockDeposit, (BlockNumber, MockBtcEvent)> = BTreeMap::new();

			for (block, event) in events {
				let deposit = event.deposit_witness();

				match chosen.get(deposit) {
					None => {
						// No event yet for this deposit, store it
						chosen.insert(deposit.clone(), (block, event));
					},
					Some((_, existing_event)) => {
						// There's already an event for this deposit
						match (existing_event, &event) {
							// If we already have a Witness, do nothing
							(MockBtcEvent::Witness(_), MockBtcEvent::PreWitness(_)) => (),
							// If we have a PreWitness and the new event is a Witness, override it
							(MockBtcEvent::PreWitness(_), MockBtcEvent::Witness(_)) => {
								chosen.insert(deposit.clone(), (block, event));
							},
							// This should be impossible to reach!
							(_, _) => (),
						}
					},
				}
			}
			chosen.into_values().collect()
		}
	}
	impl BWProcessorTypes for MockBlockProcessorDefinition {
		type ChainBlockNumber = BlockNumber;
		type BlockData = MockBlockData;
		type Event = MockBtcEvent;
		type Rules = ApplyRulesHook;
		type Execute = IncreasingHook<(Self::ChainBlockNumber, Self::Event), ()>;
		type DedupEvents = DedupEventsHook;
		type SafetyMargin = SafetyMarginHook;
	}

	#[test]
	fn test() {
		let _processor = BlockProcessor::<MockBlockProcessorDefinition>::default();
	}
}
