use crate::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	state_machine::core::Hook,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Serialize, Deserialize,
)]
pub struct DepositChannelWitnessingProcessor<T: BWProcessorTypes> {
	pub blocks_data: BTreeMap<T::ChainBlockNumber, (T::BlockData, T::ChainBlockNumber)>,
	pub reorg_events: BTreeMap<T::ChainBlockNumber, Vec<T::Event>>,
	pub rules: T::Rules,
	pub execute: T::Execute,
	pub clean_old: T::CleanOld,
	pub dedup_events: T::DedupEvents,
}
impl<BlockWitnessingProcessorDefinition: BWProcessorTypes> Default
	for DepositChannelWitnessingProcessor<BlockWitnessingProcessorDefinition>
{
	fn default() -> Self {
		Self {
			blocks_data: Default::default(),
			reorg_events: Default::default(),
			rules: Default::default(),
			execute: Default::default(),
			clean_old: Default::default(),
			dedup_events: Default::default(),
		}
	}
}

impl<T: BWProcessorTypes> DepositChannelWitnessingProcessor<T> {
	pub fn process_block_data(
		&mut self,
		chain_progress: ChainProgressInner<T::ChainBlockNumber>,
	) -> Vec<T::Event> {
		let last_block: T::ChainBlockNumber;
		match chain_progress {
			ChainProgressInner::Progress(last_height) => {
				last_block = last_height;
			},
			ChainProgressInner::Reorg(range) => {
				last_block = *range.end();
				for n in range.clone() {
					let block_data = self.blocks_data.remove(&n);
					if let Some((data, next_age)) = block_data {
						// We need to get only events already processed (next_age not included)
						for age in 0..next_age.into() {
							let events = self.process_rules_for_age_and_block(n, age.into(), &data);
							self.reorg_events.insert(n, events);
						}
					}
				}
			},
		}
		let events = self.process_rules(last_block);
		let last_events = self.dedup_events.run(events);
		for event in &last_events {
			self.execute.run(event.clone());
		}
		self.clean_old.run((&mut self.blocks_data, &mut self.reorg_events, last_block));
		last_events
	}

	pub fn insert(&mut self, n: T::ChainBlockNumber, block_data: T::BlockData) {
		self.blocks_data.insert(n, (block_data, Default::default()));
	}

	fn process_rules(&mut self, last_height: T::ChainBlockNumber) -> Vec<T::Event> {
		let mut last_events: Vec<T::Event> = vec![];
		for (block, (data, next_age)) in self.blocks_data.clone() {
			for age in next_age.into()..=last_height.into() - block.into() {
				last_events = last_events
					.into_iter()
					.chain(self.process_rules_for_age_and_block(block, age.into(), &data))
					.collect();
			}
			self.blocks_data
				.insert(block, (data.clone(), (last_height.into() - block.into() + 1).into()));
		}
		last_events
	}

	fn process_rules_for_age_and_block(
		&mut self,
		block: T::ChainBlockNumber,
		age: T::ChainBlockNumber,
		data: &T::BlockData,
	) -> Vec<T::Event> {
		let mut events: Vec<T::Event> = vec![];
		events = events.into_iter().chain(self.rules.run((block, age, data.clone()))).collect();
		events
			.into_iter()
			.filter(|last_event| {
				!self
					.reorg_events
					.iter()
					.flat_map(|(_, events)| events)
					.collect::<Vec<_>>()
					.contains(&last_event)
			})
			.collect::<Vec<_>>()
	}
}

#[cfg(test)]
pub(crate) mod test {
	use crate::{
		electoral_systems::{
			block_witnesser::{
				block_processor::DepositChannelWitnessingProcessor, state_machine::BWProcessorTypes,
			},
			state_machine::core::{hook_test_utils::IncreasingHook, Hook},
		},
		*,
	};
	use cf_chains::btc::BlockNumber;
	use codec::{Decode, Encode};
	use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
	use std::collections::BTreeMap;

	#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct MockBlockProcessorDefinition {}

	type MockBlockData = Vec<u8>;

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
	pub enum MockBtcEvent {
		PreWitness(BlockNumber, u8),
		Witness(BlockNumber, u8),
	}
	impl MockBtcEvent {
		pub fn deposit_witness(&self) -> &u8 {
			match self {
				MockBtcEvent::PreWitness(_, dw) | MockBtcEvent::Witness(_, dw) => dw,
			}
		}
		#[allow(dead_code)]
		pub fn equal_inner(&self, other: MockBtcEvent) -> bool {
			self.deposit_witness() == other.deposit_witness()
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
	impl Hook<(BlockNumber, BlockNumber, MockBlockData), Vec<MockBtcEvent>> for ApplyRulesHook {
		fn run(
			&mut self,
			(block, age, block_data): (BlockNumber, BlockNumber, MockBlockData),
		) -> Vec<MockBtcEvent> {
			// Prewitness rule
			if age == 0 {
				return block_data
					.iter()
					.map(|deposit_witness| MockBtcEvent::PreWitness(block, *deposit_witness))
					.collect::<Vec<MockBtcEvent>>();
			}
			//Full witness rule
			if age == 10 {
				return block_data
					.iter()
					.map(|deposit_witness| MockBtcEvent::Witness(block, *deposit_witness))
					.collect::<Vec<MockBtcEvent>>();
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
	pub struct CleanOldBlockDataHook {}
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
				&mut BTreeMap<BlockNumber, (MockBlockData, BlockNumber)>,
				&mut BTreeMap<BlockNumber, Vec<MockBtcEvent>>,
				BlockNumber,
			),
		) {
			blocks_data.retain(|_key, (_, age)| *age <= 5);
			reorg_events.retain(|key, _| *key > last_height - 10);
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
	impl Hook<Vec<MockBtcEvent>, Vec<MockBtcEvent>> for DedupEventsHook {
		fn run(&mut self, events: Vec<MockBtcEvent>) -> Vec<MockBtcEvent> {
			let mut chosen: BTreeMap<u8, MockBtcEvent> = BTreeMap::new();

			for event in events {
				let deposit: u8 = *event.deposit_witness();

				match chosen.get(&deposit) {
					None => {
						chosen.insert(deposit, event);
					},
					Some(existing) => match (existing, &event) {
						(MockBtcEvent::Witness(_, _), MockBtcEvent::PreWitness(_, _)) => (),
						(MockBtcEvent::PreWitness(_, _), MockBtcEvent::Witness(_, _)) => {
							chosen.insert(deposit, event);
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
		type Execute = IncreasingHook<MockBtcEvent, ()>;
		type CleanOld = CleanOldBlockDataHook;
		type DedupEvents = DedupEventsHook;
	}

	#[test]
	fn test() {
		let mut _processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();
	}
}
