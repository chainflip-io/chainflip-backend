use core::{iter::Step, ops::RangeInclusive};

use crate::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	state_machine::{
		core::{Hook, IndexOf, Indexed, Validate},
		state_machine::StateMachine,
	},
};
use cf_chains::witness_period::SaturatingStep;
use codec::{Decode, Encode};
use derive_where::derive_where;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, marker::PhantomData, vec, vec::Vec};

///
/// DepositChannelWitnessingProcessor
/// ===================================
///
/// This processor is responsible for handling block data from a blockchain deposit channel while
/// managing reorganization events (reorgs) within a safety margin. It maintains an internal state
/// of block data and reorg events, applies chain-specific processing rules (such as pre-witness and
/// witness event generation), deduplicates events to avoid processing the same deposit twice, and
/// finally executes those events.
///
/// Each blockchain can provide its own definitions for:
/// - The block number type.
/// - The block data type.
/// - The event type produced during block processing.
/// - The rules to generate events (for example, pre-witness and full witness rules).
/// - The logic for executing events.
/// - The logic for deduplicating events.
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
///     - `Execute`: A hook to execute generated events.
///     - `DedupEvents`: A hook to deduplicate events.
///     - `SafetyMargin`: A hook to retrieve the chain specific safety-margin
#[derive_where(Debug, Clone, PartialEq, Eq;
	T::ChainBlockNumber: Debug + Clone + Eq,
	T::BlockData: Debug + Clone + Eq,
	T::Event: Debug + Clone + Eq,
	T::Rules: Debug + Clone + Eq,
	T::Execute: Debug + Clone + Eq,
	T::DedupEvents: Debug + Clone + Eq,
	T::SafetyMargin: Debug + Clone + Eq,
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::BlockData: Encode,
	T::Event: Encode,
	T::Rules: Encode,
	T::Execute: Encode,
	T::DedupEvents: Encode,
	T::SafetyMargin: Encode,
))]
pub struct BlockProcessor<T: BWProcessorTypes> {
	/// A mapping from block numbers to their corresponding block data and the next age to be
	/// processed. The "age" represents the block height difference between head of the chain and
	/// block that we are processing, and it's used to know what rules have already been processed
	/// for such block
	pub blocks_data: BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>,
	pub reorg_events: BTreeMap<T::ChainBlockNumber, Vec<T::Event>>,
	pub rules: T::Rules,
	pub execute: T::Execute,
	pub dedup_events: T::DedupEvents,
	pub safety_margin: T::SafetyMargin,
}
impl<BlockWitnessingProcessorDefinition: BWProcessorTypes> Default
	for BlockProcessor<BlockWitnessingProcessorDefinition>
{
	fn default() -> Self {
		Self {
			blocks_data: Default::default(),
			reorg_events: Default::default(),
			rules: Default::default(),
			execute: Default::default(),
			dedup_events: Default::default(),
			safety_margin: Default::default(),
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
	///    - For a reorg, it removes the block data for the affected blocks and collects any events
	///      generated during that process into `reorg_events`.
	///
	/// 3. **Processing Rules:** The processor applies the chain-specific rules (via the `rules`
	///    hook) to the stored block data, generating a set of events.
	///
	/// 4. **Deduplication and Execution:** Generated events are deduplicated using the
	///    `dedup_events` hook. The remaining events are then executed via the `execute` hook.
	///
	/// # Parameters
	///
	/// - `chain_progress`: Indicates the current state of the blockchain. It can either be:
	///   - `ChainProgressInner::Progress(last_height)` for a simple progress update.
	///   - `ChainProgressInner::Reorg(range)` for a reorganization event, where `range` defines the
	///     blocks affected.
	/// - `block_data`: An optional tuple `(block_number, block_data)`. If provided, this new block
	///   data is stored.
	///
	/// # Returns
	///
	/// A vector of (block height, events (`T::Event`)) generated during the processing. These
	/// events have been deduplicated and executed.
	pub fn process_block_data(
		&mut self,
		chain_progress: ChainProgressInner<T::ChainBlockNumber>,
		block_data: Option<(T::ChainBlockNumber, T::BlockData)>,
	) -> Vec<(T::ChainBlockNumber, T::Event)> {
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
						let age_range: RangeInclusive<u32> =
							RangeInclusive::new(0, next_age.saturating_sub(1) as u32);
						let events = self
							.process_rules_for_ages_and_block(n, age_range, &data)
							.into_iter()
							.map(|(_, event)| event)
							.collect::<Vec<_>>();
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
			},
		}
		let events = self.process_rules(last_block);
		let last_events = self.dedup_events.run(events);
		for event in &last_events {
			self.execute.run(event.clone());
		}
		self.clean_old(last_block);
		last_events
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
		for (block_height, (data, next_age_to_process)) in self.blocks_data.clone() {
			let current_age = T::ChainBlockNumber::steps_between(&block_height, &last_height).0;
			let age_range: RangeInclusive<u32> =
				RangeInclusive::new(next_age_to_process, current_age as u32);
			last_events.extend(self.process_rules_for_ages_and_block(
				block_height,
				age_range,
				&data,
			));
			self.blocks_data.insert(block_height, (data.clone(), current_age as u32 + 1));
		}
		last_events
	}

	/// Applies the processing rules for a given block at a specific age to generate events.
	///
	/// This function performs two primary steps:
	///
	/// 1. **Event Generation:** It calls the `rules` hook with a tuple `(block, age, data.clone())`
	///    to generate events.
	/// 2. **Deduplication Filtering:** It then filters out events that are already present in
	///    `reorg_events`
	///
	/// # Parameters
	///
	/// - `block`: The block number for which to process rules.
	/// - `age`: The age of the block (i.e., how many blocks have passed since this block).
	/// - `data`: A reference to the block data.
	///
	/// # Returns
	///
	/// A vector of (block height, events (`T::Event`)) generated by applying the rules, excluding
	/// any duplicates.
	fn process_rules_for_ages_and_block(
		&mut self,
		block: T::ChainBlockNumber,
		age: RangeInclusive<u32>,
		data: &T::BlockData,
	) -> Vec<(T::ChainBlockNumber, T::Event)> {
		let events: Vec<(T::ChainBlockNumber, T::Event)> =
			self.rules.run((block, age, data.clone()));
		events
			.into_iter()
			.filter(|(_, last_event)| {
				!self
					.reorg_events
					.iter()
					.flat_map(|(_, events)| events)
					.any(|event| event == last_event)
			})
			.collect::<Vec<_>>()
	}
	fn clean_old(&mut self, last_height: T::ChainBlockNumber) {
		self.blocks_data
			.retain(|_key, (_, next_age)| *next_age <= self.safety_margin.run(()));
		// Todo! Do we want to keep these events around for longer? is there any benefit?
		// If we keep these for let's say 100 blocks we can then prevent double processing things
		// that are reorged up to 100 blocks later, what are the chanches of smth like this
		// happening? This still won't protect us from re-processing full Witness events since we
		// remove the blocks from block_data as soon as safety margin is reached (we would have to
		// increase the size of blocks_data as well)
		self.reorg_events.retain(|key, _| {
			key.saturating_forward(self.safety_margin.run(()) as usize) > last_height
		});
	}
}

#[cfg(test)]
pub(crate) mod test {

	use crate::{
		electoral_systems::{
			block_witnesser::{
				block_processor::BlockProcessor,
				primitives::ChainProgressInner,
				state_machine::{
					BWProcessorTypes, DedupEventsHook, ExecuteHook, HookTypeFor, RulesHook,
					SafetyMarginHook,
				},
			},
			state_machine::core::{hook_test_utils::IncreasingHook, Hook, TypesFor},
		},
		*,
	};
	use codec::{Decode, Encode};
	use core::ops::RangeInclusive;
	use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
	use std::collections::BTreeMap;

	const SAFETY_MARGIN: usize = 3;
	type BlockNumber = u64;

	pub struct MockBlockProcessorDefinition;

	type Types = TypesFor<MockBlockProcessorDefinition>;

	type MockBlockData = Vec<u8>;

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
	pub enum MockBtcEvent {
		PreWitness(u8),
		Witness(u8),
	}
	impl MockBtcEvent {
		pub fn deposit_witness(&self) -> &u8 {
			match self {
				MockBtcEvent::PreWitness(dw) | MockBtcEvent::Witness(dw) => dw,
			}
		}
	}

	impl Hook<HookTypeFor<Types, RulesHook>> for Types {
		fn run(
			&mut self,
			(block, age, block_data): (
				cf_chains::btc::BlockNumber,
				RangeInclusive<u32>,
				MockBlockData,
			),
		) -> Vec<(cf_chains::btc::BlockNumber, MockBtcEvent)> {
			let mut results: Vec<(cf_chains::btc::BlockNumber, MockBtcEvent)> = vec![];
			if age.contains(&0u32) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| {
							(block, MockBtcEvent::PreWitness(deposit_witness.clone()))
						})
						.collect::<Vec<_>>(),
				)
			}
			if age.contains(&(SAFETY_MARGIN as u32)) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| {
							(block, MockBtcEvent::Witness(deposit_witness.clone()))
						})
						.collect::<Vec<_>>(),
				)
			}
			results
		}
	}

	impl Hook<HookTypeFor<Types, DedupEventsHook>> for Types {
		fn run(
			&mut self,
			events: Vec<(BlockNumber, MockBtcEvent)>,
		) -> Vec<(BlockNumber, MockBtcEvent)> {
			// Map: deposit_witness -> chosen BtcEvent
			// todo! this is annoying, it require us to implement Ord down to the Chain type
			let mut chosen: BTreeMap<u8, (BlockNumber, MockBtcEvent)> = BTreeMap::new();

			for (block, event) in events {
				let deposit = *event.deposit_witness();

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
			chosen.into_values().collect()
		}
	}

	impl Hook<HookTypeFor<Types, SafetyMarginHook>> for Types {
		fn run(&mut self, _input: ()) -> u32 {
			3
		}
	}

	impl BWProcessorTypes for TypesFor<MockBlockProcessorDefinition> {
		type ChainBlockNumber = BlockNumber;
		type BlockData = MockBlockData;
		type Event = MockBtcEvent;
		type Rules = Types;
		type Execute = IncreasingHook<HookTypeFor<Types, ExecuteHook>>;
		type DedupEvents = Types;
		type SafetyMargin = Types;
	}

	/// tests that the processor correcly keep up to SAFETY MARGIN blocks (3), and remove them once
	/// the safety margin elapsed
	#[test]
	fn blocks_correctly_inserted_and_removed() {
		let mut processor = BlockProcessor::<Types>::default();

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

	/// temp test, checking large progress delta
	#[test]
	fn temp_large_delta() {
		let mut processor = BlockProcessor::<Types>::default();

		processor
			.process_block_data(ChainProgressInner::Progress(u32::MAX as u64), Some((9, vec![1])));
	}

	/// test that a reorg cause the processor to discard all the reorged blocks
	#[test]
	fn reorgs_remove_block_data() {
		let mut processor = BlockProcessor::<Types>::default();

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
		let mut processor = BlockProcessor::<Types>::default();

		let mut events: Vec<_> =
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
				.iter()
				.flat_map(|(block_number, events)| {
					events.iter().map(|event| (*block_number, event.clone()))
				})
				.collect::<Vec<_>>()
		);
	}

	/// test that when a reorg happens the reorged events are used to avoid re-executing the same
	/// action even if the deposit ends up in a different block,
	#[test]
	fn already_executed_events_are_not_reprocessed_after_reorg() {
		let mut processor = BlockProcessor::<Types>::default();

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
		assert_eq!(events, vec![(11, MockBtcEvent::PreWitness(10))]);
	}

	/// test that in case we process multiple action for the same deposit simultaneously
	/// (Pre-witness and Witness) we only dispactch the full deposit since it doesn't make sense to
	/// make the user pay for boost if the block was effectivily not processed in advance
	#[test]
	fn no_boost_if_full_witness_in_same_block() {
		let mut processor = BlockProcessor::<Types>::default();
		let events =
			processor.process_block_data(ChainProgressInner::Progress(15), Some((9, vec![4, 7])));

		assert_eq!(events, vec![(9, MockBtcEvent::Witness(4)), (9, MockBtcEvent::Witness(7))])
	}

	/// test that the hook executing the events is called the correct number of times
	#[test]
	fn number_of_events_executed_is_correct() {
		let mut processor = BlockProcessor::<Types>::default();

		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![6])));
		processor.process_block_data(ChainProgressInner::Progress(17), Some((16, vec![18])));

		assert_eq!(
			processor.execute.counter, 5,
			"Hook should have been called 5 times: 3 pre-witness deposit and 2 full deposit"
		)
	}
}

// State-Machine Block Witness Processor
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum SMBlockProcessorInput<T: BWProcessorTypes> {
	NewBlockData(T::ChainBlockNumber, T::ChainBlockNumber, T::BlockData),
	ChainProgress(ChainProgressInner<T::ChainBlockNumber>),
}

impl<T: BWProcessorTypes> Indexed for SMBlockProcessorInput<T> {
	type Index = ();
	fn has_index(&self, _idx: &Self::Index) -> bool {
		true
	}
}
impl<T: BWProcessorTypes> Validate for SMBlockProcessorInput<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T: BWProcessorTypes> Validate for BlockProcessor<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
#[allow(dead_code)]
pub struct SMBlockProcessorOutput<T: BWProcessorTypes>(Vec<(T::ChainBlockNumber, T::Event)>);
impl<T: BWProcessorTypes> Validate for SMBlockProcessorOutput<T> {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
pub struct SMBlockProcessor<T: BWProcessorTypes> {
	_phantom: PhantomData<T>,
}

impl<T: BWProcessorTypes + 'static> StateMachine for SMBlockProcessor<T> {
	type Input = SMBlockProcessorInput<T>;
	type Settings = ();
	type Output = SMBlockProcessorOutput<T>;
	type State = BlockProcessor<T>;

	fn input_index(_s: &mut Self::State) -> IndexOf<Self::Input> {}

	fn step(s: &mut Self::State, i: Self::Input, _set: &Self::Settings) -> Self::Output {
		match i {
			SMBlockProcessorInput::NewBlockData(last_height, n, deposits) =>
				SMBlockProcessorOutput(s.process_block_data(
					ChainProgressInner::Progress(last_height),
					Some((n, deposits)),
				)),
			SMBlockProcessorInput::ChainProgress(inner) =>
				SMBlockProcessorOutput(s.process_block_data(inner, None)),
		}
	}
}

// #[cfg(test)]
// fn step_specification(
// 	before: &Self::State,
// 	input: &Self::Input,
// 	_settings: &Self::Settings,
// 	after: &Self::State,
// ) {
// 	assert!(
// 		after.blocks_data.len() <=
// 			BitcoinIngressEgress::witness_safety_margin().unwrap() as usize,
// 		"Too many blocks data, we should never have more than safety margin blocks"
// 	);
//
// 	match input {
// 		SMBlockProcessorInput::ChainProgress(chain_progress) => match chain_progress {
// 			ChainProgressInner::Progress(_last_height) => {
// 				assert!(after.reorg_events.len() <= before.reorg_events.len(), "If no reorg happened,
// number of reorg events should stay the same or decrease"); 	// 			},
// 	// 			ChainProgressInner::Reorg(range) =>
// 	// 				for n in range.clone().into_iter() {
// 	// 					assert!(after.reorg_events.contains_key(&n), "Should always contains key for blocks
// being reorged, even if no events were produced! (Empty vec)"); 	// 					assert!(
// 	// 						!after.blocks_data.contains_key(&n),
// 	// 						"Should never contain blocks data for blocks being reorged"
// 	// 					);
// 	// 				},
// 	// 		},
// 	// 		SMBlockProcessorInput::NewBlockData(last_height, n, _deposits) => {
// 	// 			if last_height - BitcoinIngressEgress::witness_safety_margin().unwrap() > *n {
// 	// 				assert!(!after.blocks_data.contains_key(n));
// 	// 			}
// 	// 		},
// 	// 	}
// 	// }
