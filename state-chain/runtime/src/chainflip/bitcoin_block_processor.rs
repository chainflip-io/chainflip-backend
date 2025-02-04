use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::{chainflip::bitcoin_elections::BlockData, BitcoinIngressEgress, Runtime};
use cf_chains::{btc::BlockNumber, instances::BitcoinInstance};
use cf_primitives::chains::Bitcoin;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};

use log::warn;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::{block_processor::InnerEquality, state_machine::BWProcessorTypes},
	state_machine::core::Hook,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum BtcEvent {
	PreWitness(BlockNumber, DepositWitness<Bitcoin>),
	Witness(BlockNumber, DepositWitness<Bitcoin>),
}

impl BtcEvent {
	fn deposit_witness(&self) -> &DepositWitness<Bitcoin> {
		match self {
			BtcEvent::PreWitness(_, dw) | BtcEvent::Witness(_, dw) => dw,
		}
	}
	fn equal_inner(&self, other: &BtcEvent) -> bool {
		self.deposit_witness() == other.deposit_witness()
	}
}

impl InnerEquality for BtcEvent {
	fn inner_eq(&self, other: &Self) -> bool {
		self.equal_inner(other)
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
impl Hook<BtcEvent, ()> for ExecuteEventHook {
	fn run(&mut self, input: BtcEvent) {
		match input {
			BtcEvent::PreWitness(block, deposit) => {
				let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(deposit, block);
			},
			BtcEvent::Witness(block, deposit) => {
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
impl Hook<(BlockNumber, BlockNumber, BlockData), Vec<BtcEvent>> for ApplyRulesHook {
	fn run(
		&mut self,
		(block, age, block_data): (BlockNumber, BlockNumber, BlockData),
	) -> Vec<BtcEvent> {
		// Prewitness rule
		if age == 0 {
			return block_data
				.iter()
				.map(|deposit_witness| BtcEvent::PreWitness(block, deposit_witness.clone()))
				.collect::<Vec<BtcEvent>>();
		}
		//Full witness rule
		if age == BitcoinIngressEgress::witness_safety_margin().unwrap() + 5 {
			return block_data
				.iter()
				.map(|deposit_witness| BtcEvent::Witness(block, deposit_witness.clone()))
				.collect::<Vec<BtcEvent>>();
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
impl Hook<Vec<BtcEvent>, Vec<BtcEvent>> for DedupEventsHook {
	fn run(&mut self, events: Vec<BtcEvent>) -> Vec<BtcEvent> {
		// Map: deposit_witness -> chosen BtcEvent
		// todo! this is annoying, it require us to implement Ord down to the Chain type
		let mut chosen: BTreeMap<DepositWitness<Bitcoin>, BtcEvent> = BTreeMap::new();

		for event in events {
			let deposit: DepositWitness<Bitcoin> = event.deposit_witness().clone();

			match chosen.get(&deposit) {
				None => {
					// No event yet for this deposit, store it
					chosen.insert(deposit, event);
				},
				Some(existing) => {
					// There's already an event for this deposit
					match (existing, &event) {
						// If we already have a Witness, do nothing
						(BtcEvent::Witness(_, _), BtcEvent::PreWitness(_, _)) => (),
						// If we have a PreWitness and the new event is a Witness, override it
						(BtcEvent::PreWitness(_, _), BtcEvent::Witness(_, _)) => {
							chosen.insert(deposit, event);
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
pub struct CleanOldBlockDataHook {}

impl
	Hook<
		(
			&mut BTreeMap<BlockNumber, (BlockData, BlockNumber)>,
			&mut BTreeMap<BlockNumber, Vec<BtcEvent>>,
			BlockNumber,
		),
		(),
	> for CleanOldBlockDataHook
{
	fn run(
		&mut self,
		(blocks_data, reorg_events, last_height): (
			&mut BTreeMap<BlockNumber, (BlockData, BlockNumber)>,
			&mut BTreeMap<BlockNumber, Vec<BtcEvent>>,
			BlockNumber,
		),
	) {
		blocks_data.retain(|_key, (_, age)| {
			*age <= BitcoinIngressEgress::witness_safety_margin().unwrap() + 5
		});
		reorg_events.retain(|key, _| {
			*key > last_height - crate::chainflip::bitcoin_elections::BUFFER_EVENTS
		});
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
	type CleanOld = CleanOldBlockDataHook;
	type DedupEvents = DedupEventsHook;
}

#[cfg(test)]
mod tests {
	use cf_chains::btc::BlockNumber;
	use std::collections::BTreeMap;

	use crate::chainflip::bitcoin_block_processor::ApplyRulesHook;
	use codec::{Decode, Encode};
	use core::ops::RangeInclusive;
	use frame_support::pallet_prelude::TypeInfo;
	use pallet_cf_elections::electoral_systems::block_witnesser::primitives::ChainProgressInner;

	use crate::{
		chainflip::bitcoin_block_processor::{CleanOldBlockDataHook, DedupEventsHook},
		BitcoinIngressEgress,
	};
	use cf_chains::btc::BtcAmount;
	use pallet_cf_elections::electoral_systems::{
		block_witnesser::{
			block_processor::{
				DepositChannelWitnessingProcessor, InnerEquality, SMBlockProcessorInput,
			},
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
	) -> BoxedStrategy<BTreeMap<BlockNumber, (MockBlockData, BlockNumber)>> {
		prop::collection::btree_map(
			0..number_of_blocks,
			(vec![block_data()], (0..=0u64)),
			RangeInclusive::new(0, number_of_blocks as usize),
		)
		.boxed()
	}
	#[allow(dead_code)]
	fn generate_state(
	) -> BoxedStrategy<DepositChannelWitnessingProcessor<MockBlockProcessorDefinition>> {
		blocks_data(10)
			.prop_map(|data| DepositChannelWitnessingProcessor {
				blocks_data: data,
				reorg_events: Default::default(),
				rules: ApplyRulesHook {},
				execute: IncreasingHook::<MockBtcEvent, ()>::default(),
				clean_old: CleanOldBlockDataHook {},
				dedup_events: DedupEventsHook {},
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
		PreWitness(BlockNumber, MockDeposit),
		Witness(BlockNumber, MockDeposit),
	}
	impl MockBtcEvent {
		pub fn deposit_witness(&self) -> &MockDeposit {
			match self {
				MockBtcEvent::PreWitness(_, dw) | MockBtcEvent::Witness(_, dw) => dw,
			}
		}
		#[allow(dead_code)]
		pub fn equal_inner(&self, other: MockBtcEvent) -> bool {
			self.deposit_witness() == other.deposit_witness()
		}
	}
	impl InnerEquality for MockBtcEvent {
		fn inner_eq(&self, other: &Self) -> bool {
			self.deposit_witness() == other.deposit_witness()
		}
	}

	impl Hook<(BlockNumber, BlockNumber, MockBlockData), Vec<MockBtcEvent>> for ApplyRulesHook {
		fn run(
			&mut self,
			(block, age, block_data): (BlockNumber, BlockNumber, MockBlockData),
		) -> Vec<MockBtcEvent> {
			// Prewitness rule
			if age == 0 {
				return block_data
					.iter()
					.map(|deposit_witness| MockBtcEvent::PreWitness(block, deposit_witness.clone()))
					.collect::<Vec<MockBtcEvent>>();
			}
			//Full witness rule
			if age == BitcoinIngressEgress::witness_safety_margin().unwrap() {
				return block_data
					.iter()
					.map(|deposit_witness| MockBtcEvent::Witness(block, deposit_witness.clone()))
					.collect::<Vec<MockBtcEvent>>();
			}
			vec![]
		}
	}
	impl
		Hook<
			(
				&mut BTreeMap<BlockNumber, (MockBlockData, BlockNumber)>,
				&mut BTreeMap<BlockNumber, Vec<MockBtcEvent>>,
				BlockNumber,
			),
			(),
		> for CleanOldBlockDataHook
	{
		fn run(
			&mut self,
			(blocks_data, reorg_events, last_height): (
				&mut BTreeMap<BlockNumber, (Vec<MockDeposit>, BlockNumber)>,
				&mut BTreeMap<BlockNumber, Vec<MockBtcEvent>>,
				BlockNumber,
			),
		) {
			blocks_data.retain(|_key, (_, age)| *age <= 3);
			reorg_events.retain(|key, _| *key > last_height - 5);
		}
	}

	impl Hook<Vec<MockBtcEvent>, Vec<MockBtcEvent>> for DedupEventsHook {
		fn run(&mut self, events: Vec<MockBtcEvent>) -> Vec<MockBtcEvent> {
			let mut chosen: BTreeMap<MockDeposit, MockBtcEvent> = BTreeMap::new();

			for event in events {
				let deposit = event.deposit_witness();

				match chosen.get(deposit) {
					None => {
						chosen.insert(deposit.clone(), event);
					},
					Some(existing) => match (existing, &event) {
						(MockBtcEvent::Witness(_, _), MockBtcEvent::PreWitness(_, _)) => (),
						(MockBtcEvent::PreWitness(_, _), MockBtcEvent::Witness(_, _)) => {
							chosen.insert(deposit.clone(), event);
						},
						(_, _) => (),
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
		type Execute = IncreasingHook<Self::Event, ()>;
		type CleanOld = CleanOldBlockDataHook;
		type DedupEvents = DedupEventsHook;
	}

	#[test]
	fn test() {
		let _processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();
	}
}
