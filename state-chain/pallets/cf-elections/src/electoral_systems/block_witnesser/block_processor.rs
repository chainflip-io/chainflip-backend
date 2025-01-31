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

//
// #[cfg(test)]
// mod test {
//     use codec::{Decode, Encode};
//     use frame_support::{Deserialize, Serialize};
//     use frame_support::pallet_prelude::TypeInfo;
//     use cf_chains::btc::BlockNumber;
//     use crate::electoral_systems::block_witnesser::state_machine::BWProcessorTypes;
//     use crate::electoral_systems::state_machine::core::Hook;
//     use crate::electoral_systems::state_machine::core::hook_test_utils::IncreasingHook;
//
//     struct MockBlockProcessorDefinition {}
//     #[derive(
//         Clone,
//         PartialEq,
//         Eq,
//         Serialize,
//         Deserialize,
//         Encode,
//         TypeInfo,
//         Decode,
//         Debug,
//         Ord,
//         PartialOrd,
//         Default
//     )]
//     struct MockDeposit {
//         pub amount: u64,
//         pub deposit_address: String,
//     }
//     type  MockBlockData = Vec<MockDeposit>;
//
//     #[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
//     enum MockBtcEvent {
//         PreWitness(BlockNumber, MockDeposit),
//         Witness(BlockNumber, MockDeposit),
//     }
//     impl MockBtcEvent {
//         pub fn deposit_witness(&self) -> &MockDeposit {
//             match self {
//                 MockBtcEvent::PreWitness(_, dw) | MockBtcEvent::Witness(_, dw) => dw,
//             }
//         }
//         pub fn equal_inner(&self, other: MockBtcEvent) -> bool {
//             self.deposit_witness() == other.deposit_witness()
//         }
//     }
//
//     struct ApplyRulesHook{}
//     impl Hook<(BlockNumber, BlockNumber, MockBlockData), Vec<MockBtcEvent>> for ApplyRulesHook {
//         fn run(
//             &self,
//             (block, age, block_data): (BlockNumber, BlockNumber, MockBlockData),
//         ) -> Vec<MockBtcEvent> {
//             // Prewitness rule
//             if age == 0 {
//                 return block_data
//                     .iter()
//                     .map(|deposit_witness| MockBtcEvent::PreWitness(block,
// deposit_witness.clone()))                     .collect::<Vec<MockBtcEvent>>();
//             }
//             //Full witness rule
//             if age == 10 {
//                 return block_data
//                     .iter()
//                     .map(|deposit_witness| MockBtcEvent::Witness(block, deposit_witness.clone()))
//                     .collect::<Vec<MockBtcEvent>>();
//             }
//             vec![]
//         }
//     }
//     impl BWProcessorTypes for MockBlockProcessorDefinition {
//         type ChainBlockNumber = BlockNumber;
//         type BlockData = MockBlockData;
//         type Event = MockBtcEvent;
//         type Rules = ApplyRulesHook;
//         type Execute = IncreasingHook<MockBtcEvent, ()>;
//         type CleanOld = ();
//         type DedupEvents = ();
//     }
//
//     #[test]
//     fn test() {
//         let mut processor =
// DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();
//
//     }
// }
