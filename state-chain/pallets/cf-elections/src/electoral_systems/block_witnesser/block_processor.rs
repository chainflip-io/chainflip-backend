use core::{
	iter::Step,
	ops::{Range, RangeInclusive},
};

use crate::electoral_systems::{
	block_height_witnesser::{ChainBlockNumberOf, ChainTypes},
	block_witnesser::state_machine::BWProcessorTypes,
	state_machine::core::{def_derive, Hook, Validate},
};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use generic_typeinfo_derive::GenericTypeInfo;
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, vec::Vec};

def_derive! {
	///
	/// BlockProcessor
	/// ===================================
	///
	/// This processor is responsible for handling block data from a blockchain while
	/// managing reorganization events (reorgs) within a safety buffer. It maintains an internal state
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
	/// 	- `SAFETY_BUFFER`: The number of blocks to use as safety against reorgs and double processing
	///    events
	///     - `Event`: The type of event generated from processing blocks.
	///     - `Rules`: A hook to process block data and generate events.
	///     - `Execute`: A hook to dedup and execute generated events.
	/// 	- `DebugEventHook`: A hook to log events, used for testing
	#[derive(GenericTypeInfo)]
	#[expand_name_with(scale_info::prelude::format!("{}{}", T::Chain::NAME, T::BWNAME))]
	pub struct BlockProcessor<T: BWProcessorTypes> {
		/// A mapping from block numbers to their corresponding BlockInfo (block data, the next age to
		/// be processed and the safety margin). The "age" represents the block height difference
		/// between head of the chain and block that we are processing, and it's used to know what
		/// rules have already been processed for such block
		pub blocks_data: BTreeMap<ChainBlockNumberOf<T::Chain>, BlockProcessingInfo<T>>,
		/// A mapping from event to their corresponding expiration block_number (which is defined as
		/// block_number + SAFETY_BUFFER)
		pub processed_events: BTreeMap<T::Event, ChainBlockNumberOf<T::Chain>>,
		pub rules: T::Rules,
		pub execute: T::Execute,
		pub debug_events: T::DebugEventHook,
	}
}

def_derive! {
	#[derive(GenericTypeInfo)]
	#[expand_name_with(scale_info::prelude::format!("{}{}", T::Chain::NAME, T::BWNAME))]
	pub struct BlockProcessingInfo<T: BWProcessorTypes> {
		pub block_data: T::BlockData,
		pub next_age_to_process: u32,
		pub safety_margin: u32,
	}
}
impl<T: BWProcessorTypes> BlockProcessingInfo<T> {
	pub fn new(block_data: T::BlockData, safety_margin: u32) -> Self {
		BlockProcessingInfo { block_data, next_age_to_process: Default::default(), safety_margin }
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub enum BlockProcessorEvent<T: BWProcessorTypes> {
		NewBlock {
			height: ChainBlockNumberOf<T::Chain>,
			data: T::BlockData,
		},
		ProcessingBlockForAges {
			height: ChainBlockNumberOf<T::Chain>,
			ages: Range<u32>,
		},
		#[allow(clippy::type_complexity)]
		DeleteData {
			blocks: Vec<(ChainBlockNumberOf<T::Chain>, BlockProcessingInfo<T>)>,
			events: Vec<T::Event>,
		},
		StoreReorgedEvents {
			block: ChainBlockNumberOf<T::Chain>,
			events: Vec<T::Event>,
			new_block_number: ChainBlockNumberOf<T::Chain>,
		},
		UpdatingExpiry {
			event: T::Event,
			from: ChainBlockNumberOf<T::Chain>,
			to: ChainBlockNumberOf<T::Chain>,
			safety_margin: u32,
			range: RangeInclusive<ChainBlockNumberOf<T::Chain>>,
		},
	}
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
			debug_events: Default::default(),
		}
	}
}
impl<T: BWProcessorTypes> BlockProcessor<T> {
	/// Inserts new block data into the processor.
	///
	/// # Parameters
	///
	/// - `block_number`: The block number to associate with the data.
	/// - `block_data`: The data associated with the block.
	/// - `safety_margin`: The safety margin (in blocks) to use for this block.
	pub fn insert_block_data(
		&mut self,
		block_number: ChainBlockNumberOf<T::Chain>,
		block_data: T::BlockData,
		safety_margin: u32,
	) {
		self.debug_events
			.run(BlockProcessorEvent::NewBlock { height: block_number, data: block_data.clone() });
		self.blocks_data
			.insert(block_number, BlockProcessingInfo::new(block_data, safety_margin));
	}

	/// Handles chain reorganization (reorg) events by removing block data for the specified range
	/// of block heights, and marking all events generated from those blocks as already processed
	/// until a new expiry height.
	///
	/// When a reorg occurs, blocks in the `removed_block_heights` range are no longer part of the
	/// canonical chain. This method removes their data from the processor, re-generates all events
	/// that have been already produced by those blocks, and stores them in `processed_events` with
	/// an expiry set to `seen_heights_below + SAFETY_BUFFER`. This ensures that if the same events
	/// appear in new blocks due to the reorg, they are not re-processed.
	///
	/// # Parameters
	/// - `seen_heights_below`: The lowest block height that is known.
	/// - `removed_block_heights`: The inclusive range of block heights that have been removed from
	///   the canonical chain due to the reorg.
	pub fn process_reorg(
		&mut self,
		seen_heights_below: ChainBlockNumberOf<T::Chain>,
		removed_block_heights: RangeInclusive<ChainBlockNumberOf<T::Chain>>,
		safety_buffer: usize,
	) {
		let expiry = seen_heights_below.saturating_forward(safety_buffer);
		for height in removed_block_heights {
			if let Some(block_info) = self.blocks_data.remove(&height) {
				let age_range: Range<u32> = 0..block_info.next_age_to_process;

				self.rules
					.run((age_range, block_info.block_data, block_info.safety_margin))
					.iter()
					.for_each(|event| {
						self.debug_events.run(BlockProcessorEvent::StoreReorgedEvents {
							block: height,
							events: [event.clone()].into_iter().collect(),
							new_block_number: expiry,
						});
						self.processed_events.insert(event.clone(), expiry);
					});
			}
		}
	}

	/// Processes all blocks up to a given height, generating and executing new events, and cleaning
	/// up old data.
	///
	/// This method iterates over all stored block data, determines which "ages" (i.e., block height
	/// differences) need to be processed for each block, and generates new events using the
	/// chain-specific rules. It then executes all new events that have not already been processed,
	/// and updates the internal state to reflect which ages have been processed for each block.
	///
	/// After processing, it performs cleanup:
	/// - Removes block data for blocks that are now outside the safety buffer (i.e., blocks whose
	///   data is no longer needed).
	/// - Removes processed events whose expiry is below the current lowest in-progress block
	///   height.
	///
	/// # Parameters
	/// - `seen_heights_below`: The lowest block height that is known.
	/// - `lowest_in_progress_height`: The lowest block height for which there is still an ongoing
	///   election. Used to determine which processed events can be safely deleted.
	pub fn process_blocks_up_to(
		&mut self,
		seen_heights_below: ChainBlockNumberOf<T::Chain>,
		lowest_in_progress_height: ChainBlockNumberOf<T::Chain>,
		safety_buffer: usize,
	) {
		//--------- calculate new events ---------
		let new_events: Vec<_> = self
			.blocks_data
			.iter_mut()
			.flat_map(|(block_height, block_info)| {
				let new_next_age_to_process = ChainBlockNumberOf::<T::Chain>::steps_between(
					block_height,
					&seen_heights_below,
				)
				.0;
				let age_range: Range<u32> =
					(block_info.next_age_to_process)..new_next_age_to_process as u32;

				block_info.next_age_to_process = new_next_age_to_process as u32;

				self.debug_events.run(BlockProcessorEvent::ProcessingBlockForAges {
					height: *block_height,
					ages: age_range.clone(),
				});

				self.rules
					.run((age_range, block_info.block_data.clone(), block_info.safety_margin))
					.into_iter()
					.filter(|event| !self.processed_events.contains_key(event))
					.map(|event| (*block_height, event))
			})
			.collect();

		//--------- execute new events ---------
		self.execute.run(new_events);

		//--------- clean up old blocks & events ---------
		let deleted_blocks = self
			.blocks_data
			.extract_if(|block_number, _| {
				block_number.saturating_forward(safety_buffer) < seen_heights_below
			})
			.collect();

		let deleted_events = self
			.processed_events
			.extract_if(|_, expiry_block| *expiry_block < lowest_in_progress_height)
			.map(|(a, _)| a)
			.collect();

		self.debug_events.run(BlockProcessorEvent::DeleteData {
			blocks: deleted_blocks,
			events: deleted_events,
		});
	}
}

impl<T: BWProcessorTypes> Validate for BlockProcessor<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[cfg(test)]
pub(crate) mod tests {

	use crate::{
		electoral_systems::{
			block_height_witnesser::{ChainBlockHashTrait, ChainBlockNumberTrait},
			block_witnesser::{
				block_processor::BlockProcessor,
				state_machine::{
					BWProcessorTypes, BlockDataTrait, DebugEventHook, ExecuteHook, HookTypeFor,
					RulesHook,
				},
			},
			state_machine::core::{hook_test_utils::MockHook, Hook, TypesFor, Validate},
		},
		*,
	};
	use core::ops::Range;
	use frame_support::{Deserialize, Serialize};
	use proptest_derive::Arbitrary;
	use sp_std::{fmt::Debug, vec::Vec};

	const SAFETY_MARGIN: u32 = 3;
	const SAFETY_BUFFER: usize = 16;

	#[derive(
		Debug,
		Clone,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Serialize,
		Deserialize,
		Arbitrary,
		Encode,
		Decode,
		TypeInfo,
	)]
	pub enum MockBtcEvent<E> {
		PreWitness(E),
		Witness(E),
	}

	impl<
			Types: Validate + BWProcessorTypes<Event = MockBtcEvent<E>, BlockData = Vec<E>>,
			E: Clone,
		> Hook<HookTypeFor<Types, RulesHook>> for Types
	{
		fn run(
			&mut self,
			(age, block_data, safety_margin): (Range<u32>, Vec<E>, u32),
		) -> Vec<MockBtcEvent<E>> {
			let mut results: Vec<MockBtcEvent<E>> = vec![];
			if age.contains(&0u32) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| MockBtcEvent::PreWitness(deposit_witness.clone()))
						.collect::<Vec<_>>(),
				)
			}
			if age.contains(&safety_margin) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| MockBtcEvent::Witness(deposit_witness.clone()))
						.collect::<Vec<_>>(),
				)
			}
			results
		}
	}

	impl<N: ChainBlockNumberTrait, H: ChainBlockHashTrait, D: BlockDataTrait> BWProcessorTypes
		for TypesFor<(N, H, Vec<D>)>
	{
		type Chain = Self;
		type BlockData = Vec<D>;
		type Event = MockBtcEvent<D>;
		type Rules = TypesFor<(N, H, Vec<D>)>;
		type Execute = MockHook<HookTypeFor<Self, ExecuteHook>>;
		type DebugEventHook = MockHook<HookTypeFor<Self, DebugEventHook>>;

		const BWNAME: &'static str = "GenericBW";
	}

	type Types = TypesFor<(u8, Vec<u8>, Vec<u8>)>;

	/// tests that the processor correcly keep up to SAFETY_BUFFER blocks (16), and remove
	/// them once the SAFETY_BUFFER elapsed
	#[test]
	fn blocks_correctly_inserted_and_removed() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.insert_block_data(9, vec![1], SAFETY_MARGIN);
		assert_eq!(processor.blocks_data.len(), 1, "Only one blockdata added to the processor");
		processor.insert_block_data(10, vec![4], SAFETY_MARGIN);
		processor.insert_block_data(11, vec![7], SAFETY_MARGIN);
		assert_eq!(processor.blocks_data.len(), 3, "Only three blockdata added to the processor");
		for i in 0..=SAFETY_BUFFER as u8 {
			processor.insert_block_data(i, vec![i], SAFETY_MARGIN);
		}
		processor.insert_block_data(17, vec![7], SAFETY_MARGIN);

		processor.process_blocks_up_to(18, 18, SAFETY_BUFFER);
		assert_eq!(
			processor.blocks_data.len(),
			SAFETY_BUFFER,
			"Max Types::SAFETY_BUFFER (16) blocks stored at any time"
		);
	}

	/// test that a reorg cause the processor to discard all the reorged blocks
	#[test]
	fn reorgs_remove_block_data() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.insert_block_data(9, vec![1, 2, 3], SAFETY_MARGIN);
		processor.insert_block_data(10, vec![4, 5, 6], SAFETY_MARGIN);
		processor.insert_block_data(11, vec![7, 8, 9], SAFETY_MARGIN);
		processor.process_blocks_up_to(12, 12, SAFETY_BUFFER);
		processor.process_reorg(18, 9..=11, SAFETY_BUFFER);
		assert!(!processor.blocks_data.contains_key(&9));
		assert!(!processor.blocks_data.contains_key(&10));
		assert!(!processor.blocks_data.contains_key(&11));
	}

	/// test that when a reorg happens the reorged events are used to avoid re-executing the same
	/// action even if the deposit ends up in a different block,
	#[test]
	fn already_executed_events_are_not_reprocessed_after_reorg() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing (boost) for the followings deposit
		processor.insert_block_data(9, vec![1, 2], SAFETY_MARGIN);
		processor.insert_block_data(10, vec![4, 5], SAFETY_MARGIN);
		processor.process_blocks_up_to(11, 11, SAFETY_BUFFER);
		assert_eq!(
			processor.execute.take_history(),
			vec![vec![
				(9u8, MockBtcEvent::PreWitness(1u8)),
				(9u8, MockBtcEvent::PreWitness(2u8)),
				(10u8, MockBtcEvent::PreWitness(4u8)),
				(10u8, MockBtcEvent::PreWitness(5u8))
			]]
		);

		processor.process_reorg(12, 9..=11, SAFETY_BUFFER);
		processor.insert_block_data(11, vec![1, 2, 4, 5, 7], SAFETY_MARGIN);
		processor.process_blocks_up_to(12, 12, SAFETY_BUFFER);
		// After reprocessing the reorged blocks we should have not re-emitted the same prewitness
		// events for the same deposits, only the new detected deposit (7) is present

		assert_eq!(
			processor.execute.take_history(),
			vec![vec![(11u8, MockBtcEvent::PreWitness(7u8))]]
		);
	}

	/// When we encounter a reorg, already processed events are saved, with the expiration set to be
	/// the last seen block height + the SAFETY_BUFFER
	#[test]
	fn reorg_cause_processed_events_to_be_saved() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.insert_block_data(101, vec![1], SAFETY_MARGIN);
		processor.insert_block_data(102, vec![2], SAFETY_MARGIN);
		processor.process_blocks_up_to(103, 102, SAFETY_BUFFER);

		assert_eq!(processor.processed_events.len(), 0);
		processor.process_reorg(103, 101..=102, SAFETY_BUFFER);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(103u8.saturating_add(SAFETY_BUFFER as u8)).as_ref(),
		);
		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(2)),
			Some(103u8.saturating_add(SAFETY_BUFFER as u8)).as_ref(),
		);
	}

	/// When we encounter a reorg, already processed events are saved, with the expiration set to be
	/// the last seen block height + the SAFETY_BUFFER
	/// These events are deleted once latest_in_progress_height is higher than the expiry
	#[test]
	fn already_processed_events_saved_and_removed_correctly() {
		#[allow(clippy::type_complexity)]
		let mut processor: BlockProcessor<TypesFor<(u8, Vec<u8>, Vec<u8>)>> =
			BlockProcessor::<Types>::default();

		processor.insert_block_data(101, vec![1], SAFETY_MARGIN);
		processor.process_blocks_up_to(102, 101, SAFETY_BUFFER);
		processor.process_reorg(102, 101..=101, SAFETY_BUFFER);

		assert_eq!(
			processor.processed_events.get(&MockBtcEvent::PreWitness(1)),
			Some(102u8.saturating_add(SAFETY_BUFFER as u8)).as_ref(),
		);

		processor.process_blocks_up_to(
			102u8.saturating_add(SAFETY_BUFFER as u8).saturating_add(1),
			102u8.saturating_add(SAFETY_BUFFER as u8).saturating_add(1),
			SAFETY_BUFFER,
		);
		assert_eq!(processor.processed_events.get(&MockBtcEvent::PreWitness(1)), None,);
	}

	/// Using different safety margin works as expected
	#[test]
	fn dynamic_changing_safety_margin_works() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.insert_block_data(101, vec![1], SAFETY_MARGIN);

		processor.insert_block_data(102, vec![2], SAFETY_MARGIN * 2);
		processor.process_blocks_up_to(108, 108, SAFETY_BUFFER);
		//At this point we dispatch full witness only for deposit 1(safety margin 3) and not
		// 2(safety margin 6)
		assert_eq!(
			processor.execute.take_history(),
			vec![vec![
				(101u8, MockBtcEvent::PreWitness(1u8)),
				(101u8, MockBtcEvent::Witness(1u8)),
				(102u8, MockBtcEvent::PreWitness(2u8))
			]]
		);
	}

	/// Out of order blocks are correctly processed
	#[test]
	fn out_of_order_blocks_processed_correctly() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.insert_block_data(102, vec![2], SAFETY_MARGIN);
		processor.process_blocks_up_to(103, 103, SAFETY_BUFFER);
		assert_eq!(
			processor.execute.take_history(),
			vec![vec![(102u8, MockBtcEvent::PreWitness(2u8))]]
		);

		processor.insert_block_data(101, vec![1], SAFETY_MARGIN);
		processor.process_blocks_up_to(103, 103, SAFETY_BUFFER);
		assert_eq!(
			processor.execute.take_history(),
			vec![vec![(101u8, MockBtcEvent::PreWitness(1u8)),]]
		);
	}
}
