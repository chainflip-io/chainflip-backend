use core::{iter::Step, ops::Range};

use crate::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	state_machine::core::{hook_test_utils::MockHook, Hook, Validate},
};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use derive_where::derive_where;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	fmt::Debug,
	marker::PhantomData,
	vec,
	vec::Vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

///
/// BlockProcessor
/// ===================================
///
/// This processor is responsible for handling block data from a blockchain while
/// managing reorganization events (reorgs) within a safety margin. It maintains an internal state
/// of block data and already processed events, applies chain-specific processing rules (such as
/// pre-witness and witness event generation), deduplicates events to avoid processing the same
/// deposit twice, and finally executes those events.
///
/// Each block processor can provide its own definitions for:
/// - The block number type.
/// - The block data type.
/// - The event type produced during block processing.
/// - The rules to generate events (for example, pre-witness and full witness rules).
/// - The logic for executing and deduplicating events.
///
/// These are defined via the [`BWProcessorTypes`] trait, which is a generic parameter for this
/// processor.
///
/// # Type Parameters
///
/// * `T`: A type that implements [`BWProcessorTypes`]. This defines:
///     - `ChainBlockNumber`: The type representing block numbers.
///     - `BlockData`: The type of data associated with a block.
///     - `Event`: The type of event generated from processing blocks.
///     - `Rules`: A hook to process block data and generate events.
///     - `Execute`: A hook to dedup and execute generated events.
#[derive_where(Debug, Clone, PartialEq, Eq;
	T::ChainBlockNumber: Debug + Clone + Eq,
	T::BlockData: Debug + Clone + Eq,
	T::Event: Debug + Clone + Eq,
	T::Rules: Debug + Clone + Eq,
	T::Execute: Debug + Clone + Eq,
	T::LogEventHook: Debug + Clone + Eq,
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::BlockData: Encode,
	T::Event: Encode,
	T::Rules: Encode,
	T::Execute: Encode,
	T::LogEventHook: Encode,
))]
pub struct BlockProcessor<T: BWProcessorTypes> {
	/// A mapping from block numbers to their corresponding BlockInfo (block data, the next age to
	/// be processed and the safety margin). The "age" represents the block height difference
	/// between head of the chain and block that we are processing, and it's used to know what
	/// rules have already been processed for such block
	pub blocks_data: BTreeMap<T::ChainBlockNumber, BlockProcessingInfo<T::BlockData>>,
	/// A mapping from event to their corresponding expiration block_number (which is defined as
	/// block_number + safety margin)
	pub processed_events: BTreeMap<T::Event, T::ChainBlockNumber>,
	pub rules: T::Rules,
	pub execute: T::Execute,
	pub delete_data: T::LogEventHook,
}
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct BlockProcessingInfo<BlockData> {
	block_data: BlockData,
	next_age_to_process: u32,
	safety_margin: u32,
}
impl<BlockData> BlockProcessingInfo<BlockData> {
	fn new(block_data: BlockData, safety_margin: u32) -> Self {
		BlockProcessingInfo { block_data, next_age_to_process: Default::default(), safety_margin }
	}
}
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum BlockProcessorEvent<T: BWProcessorTypes> {
	DeleteBlock((T::ChainBlockNumber, BlockProcessingInfo<T::BlockData>)),
	DeleteEvents(Vec<T::Event>),
	StoreReorgedEvents {
		block: (T::ChainBlockNumber, BlockProcessingInfo<T::BlockData>),
		events: Vec<T::Event>,
		new_block_number: T::ChainBlockNumber,
	},
}

impl<BlockWitnessingProcessorDefinition: BWProcessorTypes> Default
	for BlockProcessor<BlockWitnessingProcessorDefinition>
{
	fn default() -> Self {
		Self {
			blocks_data: Default::default(),
			processed_events: Default::default(),
			rules: Default::default(),
			execute: Default::default(),
			delete_data: Default::default(),
		}
	}
}
impl<T: BWProcessorTypes> BlockProcessor<T> {
	/// Processes incoming block data and chain progress updates.
	///
	/// This method performs several key tasks:
	///
	/// 1. **Inserting Block Data:** If new block data is provided, it is inserted into the
	///    processor's state (`blocks_data`).
	///
	/// 2. **Handling Chain Progress:** Based on the provided `chain_progress`, the processor
	///    determines whether the chain has simply progressed (i.e., a new highest block) or
	///    undergone a reorganization (reorg).
	///    - For a normal progress update, it uses the latest block height to process pending block
	///      data.
	///    - For a reorg, it removes the block information for the affected blocks
	///
	/// 3. **Processing Rules:** The processor applies the chain-specific rules (via the `rules`
	///    hook) to the stored block data, generating a set of events.
	///
	/// 4. **Deduplication and Execution:** Generated events are deduplicated and then executed via
	///    the `execute` hook.
	///
	/// 5. **Cleaning:** Expired blocks and events (based on safety margin) are removed from the
	///    block processor
	///
	/// # Parameters
	///
	/// - `chain_progress`: Indicates the current state of the blockchain. It can either be:
	///   - `ChainProgressInner::Progress(last_height)` for a simple progress update.
	///   - `ChainProgressInner::Reorg(range)` for a reorganization event, where `range` defines the
	///     blocks affected.
	/// - `block_data`: An optional tuple `(block_number, block_data, safety_margin)`. If provided,
	///   this new block data is stored.
	pub fn process_block_data(
		&mut self,
		chain_progress: ChainProgressInner<T::ChainBlockNumber>,
		block_data: Option<(T::ChainBlockNumber, T::BlockData, u32)>,
	) {
		if let Some((block_number, block_data, safety_margin)) = block_data {
			self.blocks_data
				.insert(block_number, BlockProcessingInfo::new(block_data, safety_margin));
		}
		let last_block: T::ChainBlockNumber;
		match chain_progress {
			ChainProgressInner::Progress(last_height) => {
				last_block = last_height;
			},
			ChainProgressInner::Reorg(range) => {
				last_block = (*range.start()).saturating_backward(1);
				let mut highest_safety_margin = 0u32;
				for n in range.clone() {
					if let Some(block_info) = self.blocks_data.remove(&n) {
						if block_info.safety_margin > highest_safety_margin {
							highest_safety_margin = block_info.safety_margin;
						}
					}
				}
				for (_event, stored_expiry) in self.processed_events.iter_mut() {
					let mut expiry = range.end().saturating_forward(highest_safety_margin as usize);
					let new_expiry = stored_expiry.max(&mut expiry);
					*stored_expiry = *new_expiry;
				}
			},
		}
		let events = self.process_rules(last_block);
		self.execute.run(events);
		self.clean_old(last_block);
	}

	/// Processes the stored block data to generate events by applying the provided rules.
	///
	/// This method iterates over all the blocks in `blocks_data` and, for each block,
	/// applies the rules for every applicable “age” (i.e., the difference between the current block
	/// height and the block’s number). It then updates the stored "next age" for each block to
	/// ensure that future processing resumes from the correct point.
	///
	/// # Parameters
	///
	/// - `last_height`: The current highest block number in the chain.
	///
	/// # Returns
	///
	/// A vector of (block height, events (`T::Event`)) generated during the processing rules.
	fn process_rules(
		&mut self,
		last_height: T::ChainBlockNumber,
	) -> Vec<(T::ChainBlockNumber, T::Event)> {
		let mut last_events: Vec<(T::ChainBlockNumber, T::Event)> = vec![];
		for (block_height, mut block_info) in self.blocks_data.clone() {
			let new_age = T::ChainBlockNumber::steps_between(&block_height, &last_height).0;
			// We ensure that we don't break anything in case the new age < next_age_to_process
			if new_age as u32 >= block_info.next_age_to_process {
				let age_range: Range<u32> =
					(block_info.next_age_to_process)..new_age.saturating_add(1) as u32;
				last_events.extend(self.process_rules_for_ages_and_block(
					block_height,
					block_height,
					age_range,
					&block_info.block_data,
					block_info.safety_margin,
				));
				block_info.next_age_to_process = (new_age as u32).saturating_add(1);
				self.blocks_data.insert(block_height, block_info);
			}
		}
		last_events
	}

	/// Applies the processing rules for a given block and a given range of ages to generate events.
	///
	/// This function performs two primary steps:
	///
	/// 1. **Event Generation:** It calls the `rules` hook with a tuple `(block_number, age, data,
	///    safety_margin)` to generate events.
	/// 2. **Deduplication Filtering:** It then filters out events that are already present in
	///    `processed_events`. If an event is already present in `processed_events`, it is
	///    considered a duplicate. The existing entry is updated to reflect the highest expiry
	///    ChainBlockNumber between the existing and the new (duplicate) event.
	///
	/// # Parameters
	///
	/// - `block_number`: The block number for which to process rules.
	/// - `age`: The age of the block (i.e., how many blocks have passed since this block).
	/// - `data`: A reference to the block data.
	/// - `safety_margin`: the safety margin for that block
	///
	/// # Returns
	///
	/// A vector of (block height, events (`T::Event`)) generated by applying the rules, excluding
	/// any event already processed.
	fn process_rules_for_ages_and_block(
		&mut self,
		block_number: T::ChainBlockNumber,
		block_number_for_expiry: T::ChainBlockNumber,
		age: Range<u32>,
		data: &T::BlockData,
		safety_margin: u32,
	) -> Vec<(T::ChainBlockNumber, T::Event)> {
		let last_age = age.end;
		let events: Vec<(T::ChainBlockNumber, T::Event)> =
			self.rules.run((block_number, age, data.clone(), safety_margin));

		events
			.into_iter()
			.filter(|(block_number, event)| {
				let expiry = block_number
					.saturating_forward(safety_margin as usize)
					.saturating_forward(last_age as usize);
				match self.processed_events.get_mut(event) {
					Some(stored_expiry) => {
						if *stored_expiry < expiry {
							*stored_expiry = expiry;
						}
						false
					},
					None => {
						self.processed_events.insert(event.clone(), expiry);
						true
					},
				}
			})
			.collect()
	}

	fn clean_old(&mut self, last_height: T::ChainBlockNumber) {
		let removed_blocks = self.blocks_data.extract_if(|_key, block_info| {
			block_info.next_age_to_process > block_info.safety_margin
		});
		let removed_events =
			self.processed_events.extract_if(|_, expiry_block| *expiry_block <= last_height);

		for (n, block) in removed_blocks {
			self.delete_data.run(BlockProcessorEvent::DeleteBlock((n, block)));
		}
		self.delete_data
			.run(BlockProcessorEvent::DeleteEvents(removed_events.map(|(a, _)| a).collect()));
	}
}

#[cfg(test)]
pub(crate) mod tests {

	use crate::{
		electoral_systems::{
			block_height_tracking::ChainTypes,
			block_witnesser::{
				block_processor::{BlockProcessor, SMBlockProcessorInput},
				primitives::ChainProgressInner,
				state_machine::{
					BWProcessorTypes, ExecuteHook, HookTypeFor, LogEventHook, RulesHook,
				},
			},
			state_machine::core::{hook_test_utils::MockHook, Hook, Serde},
		},
		*,
	};
	use cf_chains::witness_period::{BlockZero, SaturatingStep};
	use core::{
		iter::Step,
		ops::{Range, RangeInclusive},
	};
	use frame_support::{Deserialize, Serialize};
	use proptest::prelude::Strategy;
	use proptest_derive::Arbitrary;
	use sp_std::fmt::Debug;
	use std::collections::BTreeMap;

	const SAFETY_MARGIN: u32 = 3;
	// type BlockNumber = u8;
	// type Types = (u8, Vec<u8>, MockBlockData);
	// type MockBlockData = Vec<u8>;

	#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Arbitrary)]
	pub enum MockBtcEvent<E> {
		PreWitness(E),
		Witness(E),
	}
	impl<E> MockBtcEvent<E> {
		pub fn deposit_witness(&self) -> &E {
			match self {
				MockBtcEvent::PreWitness(dw) | MockBtcEvent::Witness(dw) => dw,
			}
		}
	}

	impl<Types: BWProcessorTypes<Event = MockBtcEvent<E>, BlockData = Vec<E>>, E: Clone>
		Hook<HookTypeFor<Types, RulesHook>> for Types
	{
		fn run(
			&mut self,
			(block, age, block_data, safety_margin): (
				Types::ChainBlockNumber,
				Range<u32>,
				Vec<E>,
				u32,
			),
		) -> Vec<(Types::ChainBlockNumber, MockBtcEvent<E>)> {
			let mut results: Vec<(Types::ChainBlockNumber, MockBtcEvent<E>)> = vec![];
			if age.contains(&0u32) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| {
							(block.clone(), MockBtcEvent::PreWitness(deposit_witness.clone()))
						})
						.collect::<Vec<_>>(),
				)
			}
			if age.contains(&safety_margin) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| {
							(block.clone(), MockBtcEvent::Witness(deposit_witness.clone()))
						})
						.collect::<Vec<_>>(),
				)
			}
			results
		}
	}

	impl<Types: BWProcessorTypes<Event = MockBtcEvent<E>, BlockData = Vec<E>>, E: Clone + Ord>
		Hook<HookTypeFor<Types, ExecuteHook>> for Types
	{
		fn run(&mut self, events: Vec<(Types::ChainBlockNumber, Types::Event)>) {
			let mut chosen: BTreeMap<E, (Types::ChainBlockNumber, Types::Event)> = BTreeMap::new();

			for (block, event) in events {
				let deposit: E = event.deposit_witness().clone();

				match chosen.get(&deposit) {
					None => {
						// No event yet for this deposit, store it
						chosen.insert(deposit, (block, event));
					},
					Some((_, existing_event)) => {
						// There's already an event for this deposit
						match (existing_event, &event) {
							// If we already have a Witness, do nothing
							(MockBtcEvent::Witness(_), MockBtcEvent::PreWitness(_)) => (),
							// If we have a PreWitness and the new event is a Witness, override it
							(MockBtcEvent::PreWitness(_), MockBtcEvent::Witness(_)) => {
								chosen.insert(deposit, (block, event));
							},
							// This should be impossible to reach!
							(_, _) => (),
						}
					},
				}
			}
		}
	}

	impl<
			N: Serde
				+ Copy
				+ Ord
				+ SaturatingStep
				+ Step
				+ BlockZero
				+ sp_std::fmt::Debug
				+ Default
				+ 'static,
			H: Serde + Ord + Clone + Debug + Default + 'static,
			D: Serde + Ord + Clone + Debug + Default + 'static,
		> BWProcessorTypes for (N, H, Vec<D>)
	{
		type BlockData = Vec<D>;
		type Event = MockBtcEvent<D>;
		type Rules = (N, H, Vec<D>);
		type Execute = MockHook<HookTypeFor<Self, ExecuteHook>>;
		type LogEventHook = MockHook<HookTypeFor<Self, LogEventHook>>;
	}

	type Types = (u8, Vec<u8>, Vec<u8>);

	/// tests that the processor correcly keep up to SAFETY MARGIN blocks (3), and remove them once
	/// the safety margin elapsed
	#[test]
	fn blocks_correctly_inserted_and_removed() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((9, vec![1], SAFETY_MARGIN)),
		);
		assert_eq!(processor.blocks_data.len(), 1, "Only one blockdata added to the processor");
		processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((10, vec![4], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((11, vec![7], SAFETY_MARGIN)),
		);
		assert_eq!(processor.blocks_data.len(), 3, "Only three blockdata added to the processor");
		processor.process_block_data(
			ChainProgressInner::Progress(12),
			Some((12, vec![10], SAFETY_MARGIN)),
		);
		assert_eq!(
			processor.blocks_data.len(),
			3,
			"Max three (SAFETY MARGIN) blocks stored at any time"
		);
	}

	/// test that a reorg cause the processor to discard all the reorged blocks
	#[test]
	fn reorgs_remove_block_data() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(
			ChainProgressInner::Progress(9),
			Some((9, vec![1, 2, 3], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(10),
			Some((10, vec![4, 5, 6], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((11, vec![7, 8, 9], SAFETY_MARGIN)),
		);
		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);
		assert!(!processor.blocks_data.contains_key(&9));
		assert!(!processor.blocks_data.contains_key(&10));
		assert!(!processor.blocks_data.contains_key(&11));
	}

	///test that when a reorg happens the reorged events are used to avoid re-executing the same
	///action even if the deposit ends up in a different block,
	#[test]
	fn already_executed_events_are_not_reprocessed_after_reorg() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing (boost) for the followings deposit
		processor.process_block_data(
			ChainProgressInner::Progress(9),
			Some((9, vec![1, 2, 3], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(10),
			Some((10, vec![4, 5, 6], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(11),
			Some((11, vec![7, 8, 9], SAFETY_MARGIN)),
		);

		processor.process_block_data(ChainProgressInner::Reorg(9u8..=11), None);

		// We reprocessed the reorged blocks, now all the deposit end up in block 11
		let result = processor.process_rules_for_ages_and_block(
			11,
			11,
			0..1,
			&vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
			SAFETY_MARGIN,
		);
		// After reprocessing the reorged blocks we should have not re-emitted the same prewitness
		// events for the same deposit, only the new detected deposit (10) is present
		assert_eq!(result, vec![(11, MockBtcEvent::PreWitness(10))]);
	}

	#[test]
	fn already_executed_events_are_not_reprocessed() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing for the followings deposits
		processor.process_block_data(
			ChainProgressInner::Progress(9),
			Some((9, vec![1, 2, 3], SAFETY_MARGIN)),
		);
		// we receive next block which contains a deposit already processed (reorg detected later)
		let result =
			processor.process_rules_for_ages_and_block(10, 10, 0..1, &vec![3, 4, 5], SAFETY_MARGIN);
		// The already processed events are saved, hence only the new one are present when
		// processing the new block
		assert_eq!(
			result,
			vec![(10, MockBtcEvent::PreWitness(4)), (10, MockBtcEvent::PreWitness(5))]
		);
	}

	#[test]
	fn already_processed_events_always_have_highest_expiry_block_number() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing for the followings deposits
		processor.process_block_data(
			ChainProgressInner::Progress(9),
			Some((9, vec![1, 2, 3], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(9),
			Some((10, vec![3, 4], SAFETY_MARGIN * 2)),
		);

		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(3)),
			Some(10u8.saturating_add((SAFETY_MARGIN as u8 * 2).into()).saturating_add(1)).as_ref()
		);
	}

	// When we encounter a reorg, expiry block for all the already processed events gets bumped to
	// the max between the end of the reorg + the highest safety_margin and the currently stored
	// expiry
	#[test]
	fn reorg_cause_expiry_block_to_be_bumped() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(
			ChainProgressInner::Progress(101),
			Some((101, vec![1], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(102),
			Some((102, vec![], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(103),
			Some((103, vec![], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(104),
			Some((104, vec![], SAFETY_MARGIN)),
		);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(101u8.saturating_add((SAFETY_MARGIN as u8).into()).saturating_add(1)).as_ref(),
		);
		processor
			.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(101, 105)), None);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(105u8.saturating_add((SAFETY_MARGIN as u8).into())).as_ref(),
		);
	}

	// When we encounter an event already processed we update its expiry to be the max between
	// last_seen_height + safety_margin and the currently stored expiry
	#[test]
	fn already_processed_events_expiry_is_updated_based_on_last_seen_height() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(
			ChainProgressInner::Progress(101),
			Some((101, vec![1, 2], SAFETY_MARGIN)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(102),
			Some((102, vec![3], SAFETY_MARGIN)),
		);
		processor.process_block_data(ChainProgressInner::Progress(103), None);

		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(101u8.saturating_add((SAFETY_MARGIN as u8).into()).saturating_add(1)).as_ref(),
		);
		processor
			.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(101, 103)), None);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(103u8.saturating_add((SAFETY_MARGIN as u8).into())).as_ref(),
		);

		processor.process_block_data(
			ChainProgressInner::Progress(104),
			Some((101u8, vec![1], SAFETY_MARGIN)),
		);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(104u8.saturating_add((SAFETY_MARGIN as u8).into()).saturating_add(1)).as_ref(),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(104),
			Some((102, vec![2], SAFETY_MARGIN)),
		);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(2)),
			Some(104u8.saturating_add((SAFETY_MARGIN as u8).into()).saturating_add(1)).as_ref(),
		);
	}

	// The expiry block cannot be lowered, and the highest value is always kept, even in case we
	// update the safety margin
	#[test]
	fn change_in_safety_margin_do_not_impact_expiry_block() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(
			ChainProgressInner::Progress(101),
			Some((101, vec![1, 2], SAFETY_MARGIN * 2)),
		);
		processor.process_block_data(
			ChainProgressInner::Progress(102),
			Some((102, vec![3], SAFETY_MARGIN * 2)),
		);
		processor.process_block_data(ChainProgressInner::Progress(103), None);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(101u8.saturating_add((SAFETY_MARGIN as u8 * 2).into()).saturating_add(1)).as_ref(),
		);
		processor
			.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(101, 103)), None);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(103u8.saturating_add((SAFETY_MARGIN as u8 * 2).into())).as_ref(),
		);
		processor.process_block_data(ChainProgressInner::Progress(101), Some((101, vec![1, 2], 1)));
		processor.process_block_data(ChainProgressInner::Progress(102), Some((102, vec![3], 1)));
		processor.process_block_data(ChainProgressInner::Progress(103), None);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(103u8.saturating_add((SAFETY_MARGIN as u8 * 2).into())).as_ref(),
		);
		println!("{:?}", processor.processed_events);
	}
}

// State-Machine Block Witness Processor
#[cfg_attr(test, derive(Arbitrary))]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum SMBlockProcessorInput<T: BWProcessorTypes> {
	NewBlockData(T::ChainBlockNumber, T::ChainBlockNumber, T::BlockData),
	ChainProgress(ChainProgressInner<T::ChainBlockNumber>),
}

impl<T: BWProcessorTypes> Validate for BlockProcessor<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
#[allow(dead_code)]
pub struct SMBlockProcessorOutput<T: BWProcessorTypes> {
	events: Vec<(T::ChainBlockNumber, T::Event)>,
	deleted_data: BTreeMap<T::ChainBlockNumber, BlockProcessingInfo<T::BlockData>>,
	deleted_events: Vec<(T::ChainBlockNumber, (Vec<T::Event>, u32))>,
}
impl<T: BWProcessorTypes> Validate for SMBlockProcessorOutput<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
pub struct SMBlockProcessor<T: BWProcessorTypes> {
	_phantom: PhantomData<T>,
}

use crate::electoral_systems::state_machine::core::IndexedValidate;
impl<T: BWProcessorTypes + 'static + Debug>
	IndexedValidate<BTreeSet<T::ChainBlockNumber>, SMBlockProcessorInput<T>> for SMBlockProcessor<T>
{
	type Error = ();
	fn validate(
		index: &BTreeSet<T::ChainBlockNumber>,
		value: &SMBlockProcessorInput<T>,
	) -> Result<(), Self::Error> {
		match value {
			SMBlockProcessorInput::NewBlockData(_, n, _) =>
				if index.contains(n) {
					Err(())
				} else {
					Ok(())
				},
			SMBlockProcessorInput::ChainProgress(_) => Ok(()),
		}
	}
}

use crate::electoral_systems::state_machine::state_machine::Statemachine;

use super::state_machine::{ExecuteHook, HookTypeFor, LogEventHook};
impl<
		T: BWProcessorTypes<
				LogEventHook = MockHook<HookTypeFor<T, LogEventHook>>,
				Execute = MockHook<HookTypeFor<T, ExecuteHook>>,
			>
			+ 'static
			+ Debug
			+ Clone
			+ Eq,
	> Statemachine for SMBlockProcessor<T>
{
	type Input = SMBlockProcessorInput<T>;
	type InputIndex = BTreeSet<T::ChainBlockNumber>;
	type Settings = u32;
	type Output = ();
	type State = BlockProcessor<T>;

	fn input_index(s: &mut Self::State) -> Self::InputIndex {
		s.blocks_data.keys().cloned().collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, set: &Self::Settings) -> Self::Output {
		match i {
			SMBlockProcessorInput::NewBlockData(last_height, n, deposits) => s.process_block_data(
				ChainProgressInner::Progress(last_height),
				Some((n, deposits, *set)),
			),
			SMBlockProcessorInput::ChainProgress(inner) => s.process_block_data(inner, None),
		}
	}

	#[cfg(test)]
	fn step_specification(
		pre: &mut Self::State,
		input: &Self::Input,
		_output: &Self::Output,
		settings: &Self::Settings,
		post: &Self::State,
	) {
		use crate::{
			asserts,
			electoral_systems::{
				block_height_tracking::ChainTypes,
				block_witnesser::helpers::Merge,
				state_machine::test_utils::{BTreeMultiSet, Container},
			},
		};
		use std::collections::BTreeSet;

		type BlocksData<T> = BTreeMap<
			<T as ChainTypes>::ChainBlockNumber,
			BlockProcessingInfo<<T as BWProcessorTypes>::BlockData>,
		>;

		type Multiset<A> = Container<BTreeSet<A>>;

		let active_events = |s: &BlocksData<T>| -> Multiset<T::Event> {
			s.iter()
				.flat_map(|(height, block_info)| {
					let mut x: BlockProcessor<T> = Default::default();
					x.rules.run((
						*height,
						(0..block_info.next_age_to_process),
						block_info.block_data.clone(),
						block_info.safety_margin,
					))
				})
				.map(|(_number, event)| event)
				.collect()
		};

		let history = &post.delete_data.call_history;
		let deleted_blocks: BTreeMap<_, _> = history
			.iter()
			.filter_map(|event| match event {
				BlockProcessorEvent::DeleteBlock(block) => Some(block),
				_ => None,
			})
			.cloned()
			.collect();

		let deleted_events: Container<_> = history
			.iter()
			.filter_map(|event| match event {
				BlockProcessorEvent::DeleteEvents(events) => Some(events),
				_ => None,
			})
			.cloned()
			.flatten()
			.collect();

		let stored_executed_events = &post.execute.call_history;

		let events = |s: &Self::State| {
			active_events(&s.blocks_data) + s.processed_events.keys().cloned().collect()
		};
		let deleted_events = active_events(&deleted_blocks) + deleted_events;

		let executed_events = || -> Multiset<_> {
			stored_executed_events.iter().flatten().map(|(_, v)| v).cloned().collect()
		};
		let executed_events_vector = || -> Container<BTreeMultiSet<_>> {
			stored_executed_events.iter().flatten().map(|(_k, v)| v).cloned().collect()
		};

		let reorg =
			matches!(input, SMBlockProcessorInput::ChainProgress(ChainProgressInner::Reorg(_)));
		let blocks = |d: &BlocksData<T>| {
			d.values()
				.map(|block_info| block_info.block_data.clone())
				.collect::<BTreeSet<_>>()
		};

		let deleted_new: BTreeSet<T::BlockData> = match input {
			SMBlockProcessorInput::NewBlockData(n, i, x)
				if i.saturating_forward(*settings as usize) <= *n =>
				BTreeSet::from([x.clone()]),
			_ => BTreeSet::new(),
		};

		let new_block: BTreeSet<T::BlockData> = match input {
			SMBlockProcessorInput::NewBlockData(_n, _i, x) => BTreeSet::from([x.clone()]),
			_ => BTreeSet::new(),
		};

		/*
		asserts! {

			"the executed events are exactly those that are new (post events: {:?}, pre: {:?}, executed: {:?}, post-state: {:?})"
			in events(post) + deleted_events == executed_events() + events(pre),
			else
				events(post),
				events(pre),
				executed_events(),
				post
			;

			"stored events are never executed again"
			in events(pre) & executed_events() == Container(BTreeSet::new());

			"executed events are unique"
			in executed_events_vector().0.0.iter().all(|(_x, n)| *n == 1);

			// TODO: handle the reorg case
			"blocks either stay in the blockstore or are included in the 'deleted output'"
			in if !reorg {
				blocks(&pre.blocks_data).is_subset(&blocks(&post.blocks_data).merge(blocks(&deleted_blocks)))
			} else {true};

			"new blocks are added to block data or are immediately deleted"
			in new_block.is_subset(&blocks(&post.blocks_data).merge(deleted_new));
		}
		*/
	}
}
