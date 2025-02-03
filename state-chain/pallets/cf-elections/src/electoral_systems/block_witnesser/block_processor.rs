use crate::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	state_machine::core::Hook,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, vec, vec::Vec};


/// Block Processor
/// responsible for processing blocks while handling reorgs based on our safety margin
/// Blocks data are inserted in the processor along with the current height of the chain (which is used to determine which rules to process)
///
/// Each chain will implement its own rules (I.E. Pre-witness and Witness for BTC)
///	When to delete block data
/// How to dedup events (I.E. in case of both Pre-witness and Witness events in the same iteration)
///
pub trait InnerEquality {
	fn inner_eq(&self, other: &Self) -> bool;
}

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
		block_data: Option<(T::ChainBlockNumber, T::BlockData)>,
	) -> Vec<T::Event> {
		if let Some((block_number, block_data)) = block_data {
			self.blocks_data.insert(block_number, (block_data, Default::default()));
		}
		let last_block: T::ChainBlockNumber;
		match chain_progress {
			ChainProgressInner::Progress(last_height) => {
				last_block = last_height;
			},
			ChainProgressInner::Reorg(range) => {
				last_block = *range.end();
				for n in range {
					let block_data = self.blocks_data.remove(&n);
					if let Some((data, next_age)) = block_data {
						// We need to get only events already processed (next_age not included)
						for age in 0..next_age.into() {
							let events = self.process_rules_for_age_and_block(n, age.into(), &data);
							match self.reorg_events.get_mut(&n) {
								None => {
									self.reorg_events.insert(n, events);
								},
								Some(previous_events) => {
									previous_events.extend(events.into_iter());
								},
							}
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

	fn process_rules(&mut self, last_height: T::ChainBlockNumber) -> Vec<T::Event> {
		let mut last_events: Vec<T::Event> = vec![];
		for (block, (data, next_age)) in self.blocks_data.clone() {
			for age in next_age.into()..=last_height.into().saturating_sub(block.into()) {
				last_events = last_events
					.into_iter()
					.chain(self.process_rules_for_age_and_block(block, age.into(), &data))
					.collect();
			}
			self.blocks_data.insert(
				block,
				(data.clone(), (last_height.into().saturating_sub(block.into()) + 1).into()),
			);
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
					.any(|event| event.inner_eq(last_event))
			})
			.collect::<Vec<_>>()
	}
}

#[cfg(test)]
pub(crate) mod test {
	use crate::{
		electoral_systems::{
			block_witnesser::{
				block_processor::{DepositChannelWitnessingProcessor, InnerEquality},
				primitives::ChainProgressInner,
				state_machine::BWProcessorTypes,
			},
			state_machine::core::{hook_test_utils::IncreasingHook, Hook},
		},
		*,
	};
	use cf_chains::btc::BlockNumber;
	use codec::{Decode, Encode};
	use core::ops::RangeInclusive;
	use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
	use std::collections::BTreeMap;

	const SAFETY_MARGIN: u64 = 3;
	const BUFFER_REORG_EVENTS: u64 = 5;
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
		pub fn equal_inner(&self, other: &MockBtcEvent) -> bool {
			self.deposit_witness() == other.deposit_witness()
		}
	}

	impl InnerEquality for MockBtcEvent {
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
			if age == SAFETY_MARGIN {
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
			blocks_data.retain(|_key, (_, age)| *age <= SAFETY_MARGIN);
			reorg_events.retain(|key, _| *key > last_height - BUFFER_REORG_EVENTS);
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

	/// tests that the processor correcly keep up to SAFETY MARGIN blocks (3), and remove them once
	/// the safety margin elapsed
	#[test]
	fn blocks_correctly_inserted_and_removed() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();

		processor.process_block_data(ChainProgressInner::Progress(11), Some((9, vec![1])));
		assert_eq!(processor.blocks_data.len(), 1, "Only one blockdata added to the processor");
		processor.process_block_data(ChainProgressInner::Progress(11), Some((10, vec![4])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7])));
		assert_eq!(processor.blocks_data.len(), 3, "Only three blockdata added to the processor");
		processor.process_block_data(ChainProgressInner::Progress(12), Some((12, vec![10])));
		assert_eq!(
			processor.blocks_data.len(),
			3,
			"Max three (SAFETY MARGIN) blocks stored at any time"
		);
	}

	/// test that a reorg cause the processor to discard all the reorged blocks
	#[test]
	fn reorgs_remove_block_data() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();

		processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1, 2, 3])));
		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4, 5, 6])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7, 8, 9])));
		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);
		assert!(!processor.blocks_data.contains_key(&9));
		assert!(!processor.blocks_data.contains_key(&10));
		assert!(!processor.blocks_data.contains_key(&11));
	}

	/// test that a reorg is properly handled by saving all the events executed so far
	#[test]
	fn reorgs_events_saved_and_removed() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();

		let mut events =
			processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1, 2, 3])));
		events.extend(
			processor
				.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4, 5, 6]))),
		);
		events.extend(
			processor
				.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7, 8, 9]))),
		);
		//when a reorg happens the block processor saves all the events it has processed so far for
		// the reorged blocks
		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);
		assert_eq!(
			events,
			processor
				.reorg_events
				.into_iter()
				.flat_map(|(_, event)| event)
				.collect::<Vec<_>>()
		);
	}

	/// test that when a reorg happens the reorged events are used to avoid re-executing the same
	/// action even if the deposit ends up in a different block, we have a BUFFER (5) that dictates
	/// for how many blocks these events will be kept in the processor
	#[test]
	fn already_executed_events_are_not_reprocessed_after_reorg() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();

		// We processed pre-witnessing (boost) for the followings deposit
		processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1, 2, 3])));
		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4, 5, 6])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7, 8, 9])));
		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);
		// We reprocessed the reorged blocks, now all the deposit end up in block 11
		let mut events =
			processor.process_block_data(ChainProgressInner::Progress(11), Some((9, vec![])));
		events.extend(
			processor.process_block_data(ChainProgressInner::Progress(11), Some((10, vec![]))),
		);
		events.extend(processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((11, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])),
		));
		// After reprocessing the reorged blocks we should have not re-emitted the same prewitness
		// events for the same deposit, only the new detected deposit (10) is present
		assert_eq!(events, vec![MockBtcEvent::PreWitness(11, 10)]);
	}

	/// test that in case we process multiple action for the same deposit simultaneously
	/// (Pre-witness and Witness) we only dispactch the full deposit since it doesn't make sense to
	/// make the user pay for boost if the block was effectivily not processed in advance
	#[test]
	fn no_boost_if_full_witness_in_same_block() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();
		let events =
			processor.process_block_data(ChainProgressInner::Progress(15), Some((9, vec![4, 7])));

		assert_eq!(events, vec![MockBtcEvent::Witness(9, 4), MockBtcEvent::Witness(9, 7)])
	}

	/// test that the hook executing the events is called the correct number of times
	#[test]
	fn number_of_events_executed_is_correct() {
		let mut processor =
			DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();

		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![6])));
		processor.process_block_data(ChainProgressInner::Progress(17), Some((16, vec![18])));

		assert_eq!(
			processor.execute.counter, 5,
			"Hook should have been called 5 times: 3 pre-witness deposit and 2 full deposit"
		)
	}
}
