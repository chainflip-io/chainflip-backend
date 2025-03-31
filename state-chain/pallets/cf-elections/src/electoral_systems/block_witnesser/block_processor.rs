use core::{iter::Step, ops::Range};

use crate::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	state_machine::core::{Hook, Validate},
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

type BlockStore<T: BWProcessorTypes> = BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>;

pub fn past_events<T: BWProcessorTypes>(store: &mut BlockStore<T>) -> Vec<T::Event> {
	store
		.iter()
		.flat_map(|(height, (data, age))| {
			let mut x: BlockProcessor<T> = Default::default();
			x.rules.run((*height, (0..*age), data.clone()))
		})
		.map(|(number, event)| event)
		.collect()
}

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
///     - `Execute`: A hook to execute generated events.
///     - `SafetyMargin`: A hook to retrieve the chain specific safety-margin
#[derive_where(Debug, Clone, PartialEq, Eq;
	T::ChainBlockNumber: Debug + Clone + Eq,
	T::BlockData: Debug + Clone + Eq,
	T::Event: Debug + Clone + Eq,
	T::Rules: Debug + Clone + Eq,
	T::Execute: Debug + Clone + Eq,
	T::SafetyMargin: Debug + Clone + Eq,
)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound(
	T::ChainBlockNumber: Encode,
	T::BlockData: Encode,
	T::Event: Encode,
	T::Rules: Encode,
	T::Execute: Encode,
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
	/// 4. **Deduplication and Execution:** Generated events are deduplicated and then executed via
	///    the `execute` hook.
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
	) -> (
		Vec<(T::ChainBlockNumber, T::Event)>,
		(
			Vec<(T::ChainBlockNumber, Vec<T::Event>)>,
			BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>,
		),
	) {
		let mut executed_events: Vec<_> = Vec::new();
		// events in reorged events
		for (n, events) in &self.reorg_events {
			executed_events.append(&mut events.clone());
		}
		// events in block data
		for (n, (data, next_age)) in &self.blocks_data {
			executed_events.extend(
				self.rules.run((*n, (0..*next_age), data.clone())).into_iter().map(|(n, x)| x),
			);
		}

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

				self.blocks_data
					.extract_if(|n, _| range.contains(n))
					.collect::<Vec<_>>()
					.into_iter()
					.for_each(|(n, (data, next_age))| {
						let age_range: Range<u32> = 0..next_age;
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
					});
			},
		}
		let events = self.process_rules(last_block);
		self.execute.run(
			events
				.iter()
				.cloned()
				.filter(|(n, event)| !executed_events.contains(event))
				.collect(),
		);
		// for (n, event) in &events {
		// 	self.reorg_events.entry(*n).or_insert(Vec::new()).extend([event.clone()]);
		// }
		(events, self.clean_old(last_block))
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
			let new_age = T::ChainBlockNumber::steps_between(&block_height, &last_height).0;
			// We ensure that we don't break anything in case the new age < next_age_to_process
			if new_age as u32 >= next_age_to_process {
				let age_range: Range<u32> = next_age_to_process..new_age.saturating_add(1) as u32;
				last_events.extend(self.process_rules_for_ages_and_block(
					block_height,
					age_range,
					&data,
				));
				self.blocks_data
					.insert(block_height, (data.clone(), (new_age as u32).saturating_add(1)));
			}
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
	///    `reorg_events` or in the active blocks themselves.
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
		age: Range<u32>,
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
					.any(|event| event == last_event) &&
					!past_events::<T>(&mut self.blocks_data).contains(last_event)
			})
			.collect::<Vec<_>>()
	}
	fn clean_old(
		&mut self,
		last_height: T::ChainBlockNumber,
	) -> (
		Vec<(T::ChainBlockNumber, Vec<T::Event>)>,
		BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>,
	) {
		// TODO 2025-03-26: to-discuss: currently the deletion of blocks is done
		// by safety-margin. There is an implicit requirement that the rules-hook
		// should only generate events for ages <= safety_margin which isn't checked
		// or guaranteed anywhere.
		let blocks = self
			.blocks_data
			.extract_if(|_key, (_, next_age)| *next_age > self.safety_margin.run(()))
			.collect();
		// Todo! Do we want to keep these events around for longer? is there any benefit?
		// If we keep these for let's say 100 blocks we can then prevent double processing things
		// that are reorged up to 100 blocks later, what are the chanches of smth like this
		// happening? This still won't protect us from re-processing full Witness events since we
		// remove the blocks from block_data as soon as safety margin is reached (we would have to
		// increase the size of blocks_data as well)
		let events = self
			.reorg_events
			.extract_if(|key, _| {
				key.saturating_forward(self.safety_margin.run(()) as usize) <= last_height
			})
			.collect();

		(events, blocks)
	}
}

#[cfg(test)]
pub(crate) mod tests {

	use crate::{
		electoral_systems::{
			block_witnesser::{
				block_processor::{BlockProcessor, SMBlockProcessorInput},
				primitives::ChainProgressInner,
				state_machine::{
					BWProcessorTypes, ExecuteHook, HookTypeFor, RulesHook, SafetyMarginHook,
				},
			},
			state_machine::core::{hook_test_utils::MockHook, Hook, IndexedValidate, TypesFor},
		},
		*,
	};
	use codec::{Decode, Encode};
	use core::ops::{Range, RangeInclusive};
	use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
	use proptest::{arbitrary::arbitrary, prelude::Strategy};
	use proptest_derive::Arbitrary;
	use std::collections::BTreeMap;

	const SAFETY_MARGIN: usize = 3;
	type BlockNumber = u8;

	pub struct MockBlockProcessorDefinition;

	type Types = TypesFor<MockBlockProcessorDefinition>;

	type MockBlockData = Vec<u8>;

	#[derive(
		Arbitrary,
		Debug,
		Clone,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Encode,
		Decode,
		TypeInfo,
		Deserialize,
		Serialize,
	)]
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
			(block, age, block_data): (BlockNumber, Range<u32>, MockBlockData),
		) -> Vec<(BlockNumber, MockBtcEvent)> {
			let mut results: Vec<(BlockNumber, MockBtcEvent)> = vec![];
			if age.contains(&0u32) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| (block, MockBtcEvent::PreWitness(*deposit_witness)))
						.collect::<Vec<_>>(),
				)
			}
			if age.contains(&(SAFETY_MARGIN as u32)) {
				results.extend(
					block_data
						.iter()
						.map(|deposit_witness| (block, MockBtcEvent::Witness(*deposit_witness)))
						.collect::<Vec<_>>(),
				)
			}
			results
		}
	}

	impl Hook<HookTypeFor<Types, SafetyMarginHook>> for Types {
		fn run(&mut self, _input: ()) -> u32 {
			3
		}
	}

	impl Hook<HookTypeFor<Types, ExecuteHook>> for Types {
		fn run(&mut self, events: Vec<(BlockNumber, MockBtcEvent)>) {
			let mut chosen: BTreeMap<u8, (BlockNumber, MockBtcEvent)> = BTreeMap::new();

			for (block, event) in events {
				let deposit: u8 = *event.deposit_witness();

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

	impl BWProcessorTypes for TypesFor<MockBlockProcessorDefinition> {
		type ChainBlockNumber = BlockNumber;
		type BlockData = MockBlockData;
		type Event = MockBtcEvent;
		type Rules = Types;
		// type Execute = Types;
		type Execute = MockHook<HookTypeFor<Types, ExecuteHook>>;
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

	///temp test, checking large progress delta
	#[test]
	fn temp_large_delta() {
		let mut processor = BlockProcessor::<Types>::default();

		// processor
		// 	.process_block_data(ChainProgressInner::Progress(u32::MAX as u64), Some((9, vec![1])));
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

		processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1, 2, 3])));
		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4, 5, 6])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7, 8, 9])));

		//when a reorg happens the block processor saves all the events it has processed so far for
		// the reorged blocks
		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);
		assert_eq!(
			vec![
				(9, MockBtcEvent::PreWitness(1)),
				(9, MockBtcEvent::PreWitness(2)),
				(9, MockBtcEvent::PreWitness(3)),
				(10, MockBtcEvent::PreWitness(4)),
				(10, MockBtcEvent::PreWitness(5)),
				(10, MockBtcEvent::PreWitness(6)),
				(11, MockBtcEvent::PreWitness(7)),
				(11, MockBtcEvent::PreWitness(8)),
				(11, MockBtcEvent::PreWitness(9))
			],
			processor
				.reorg_events
				.iter()
				.flat_map(|(block_number, events)| {
					events.iter().map(|event| (*block_number, event.clone()))
				})
				.collect::<Vec<_>>()
		);
		processor.process_block_data(ChainProgressInner::Progress(13), None);
		assert_eq!(
			vec![
				(11, MockBtcEvent::PreWitness(7)),
				(11, MockBtcEvent::PreWitness(8)),
				(11, MockBtcEvent::PreWitness(9))
			],
			processor
				.reorg_events
				.iter()
				.flat_map(|(block_number, events)| {
					events.iter().map(|event| (*block_number, event.clone()))
				})
				.collect::<Vec<_>>()
		);
		processor.process_block_data(ChainProgressInner::Progress(14), None);
		assert!(processor.reorg_events.is_empty())
	}

	///test that when a reorg happens the reorged events are used to avoid re-executing the same
	///action even if the deposit ends up in a different block,
	#[test]
	fn already_executed_events_are_not_reprocessed_after_reorg() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing (boost) for the followings deposit
		processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1, 2, 3])));
		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![4, 5, 6])));
		processor.process_block_data(ChainProgressInner::Progress(11), Some((11, vec![7, 8, 9])));

		processor.process_block_data(ChainProgressInner::Reorg(RangeInclusive::new(9, 11)), None);

		// We reprocessed the reorged blocks, now all the deposit end up in block 11
		let result = processor.process_rules_for_ages_and_block(
			11,
			0..1,
			&vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
		);
		// After reprocessing the reorged blocks we should have not re-emitted the same prewitness
		// events for the same deposit, only the new detected deposit (10) is present
		assert_eq!(result, vec![(11, MockBtcEvent::PreWitness(10))]);
	}

	#[test]
	fn same_block_twice() {
		let mut processor = BlockProcessor::<Types>::default();
		// We processed pre-witnessing (boost) for the followings deposit
		processor.process_block_data(ChainProgressInner::Progress(9), Some((9, vec![1])));
		processor.process_block_data(ChainProgressInner::Progress(10), Some((10, vec![1])));
	}

	// ------------------------ fuzzy testing ---------------------------

	use proptest::prelude::*;
	pub fn generate_state() -> impl Strategy<Value = BlockProcessor<Types>> {
		// Precondition: block_data should have only unique entries
		let block_data = proptest::collection::btree_set(any::<u8>(), 0..10)
			.prop_map(|set| set.into_iter().collect::<Vec<_>>());
		(
			proptest::collection::btree_map(any::<BlockNumber>(), (block_data, 0..10u32), 0..10),
			proptest::collection::btree_map(
				any::<BlockNumber>(),
				proptest::collection::vec(any::<MockBtcEvent>(), 0..10),
				0..10,
			),
		)
			.prop_map(|(blocks_data, reorg_events)| BlockProcessor {
				blocks_data,
				reorg_events,
				..Default::default()
			})
	}

	pub fn generate_input() -> impl Strategy<Value = SMBlockProcessorInput<Types>> {
		prop_oneof![
			((
				any::<u8>(),
				any::<u8>(),
				proptest::collection::btree_set(any::<u8>(), 0..10)
					.prop_map(|set| set.into_iter().collect::<Vec<_>>())
			))
				.prop_map(|(x, y, z)| SMBlockProcessorInput::NewBlockData(x, y, z)),
			any::<ChainProgressInner<u8>>().prop_map(SMBlockProcessorInput::ChainProgress)
		]
	}

	#[test]
	pub fn test_block_processor() {
		use super::SMBlockProcessor;
		use crate::electoral_systems::state_machine::state_machine::Statemachine;
		use proptest::{
			prelude::{any, prop, Arbitrary, Just, Strategy},
			prop_oneof,
			sample::select,
		};

		SMBlockProcessor::<Types>::test(module_path!(), generate_state(), Just(()), |indices| {
			generate_input()
				.prop_filter("..", move |input| match input {
					SMBlockProcessorInput::NewBlockData(_, n, _) => !indices.contains(n),
					SMBlockProcessorInput::ChainProgress(_) => true,
				})
				.boxed()
		})
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
	deleted_data: BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>,
	deleted_events: Vec<(T::ChainBlockNumber, Vec<T::Event>)>,
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
impl<T: BWProcessorTypes + 'static + Debug> Statemachine for SMBlockProcessor<T> {
	type Input = SMBlockProcessorInput<T>;
	type InputIndex = BTreeSet<T::ChainBlockNumber>;
	type Settings = ();
	type Output = SMBlockProcessorOutput<T>;
	type State = BlockProcessor<T>;

	fn input_index(s: &mut Self::State) -> Self::InputIndex {
		s.blocks_data.keys().cloned().collect()
	}

	fn step(s: &mut Self::State, i: Self::Input, _set: &Self::Settings) -> Self::Output {
		let (events, (a, b)) = match i {
			SMBlockProcessorInput::NewBlockData(last_height, n, deposits) =>
				s.process_block_data(ChainProgressInner::Progress(last_height), Some((n, deposits))),
			SMBlockProcessorInput::ChainProgress(inner) => s.process_block_data(inner, None),
		};
		SMBlockProcessorOutput { events, deleted_data: b, deleted_events: a }
	}

	#[cfg(test)]
	fn step_specification(
		pre: &mut Self::State,
		input: &Self::Input,
		output: &Self::Output,
		_settings: &Self::Settings,
		post: &Self::State,
	) {
		use crate::{
			asserts,
			electoral_systems::{
				block_witnesser::helpers::Merge,
				state_machine::test_utils::{BTreeMultiSet, Container},
			},
		};
		use std::collections::BTreeSet;

		type BlocksData<T: BWProcessorTypes> = BTreeMap<T::ChainBlockNumber, (T::BlockData, u32)>;
		type ReorgData<T: BWProcessorTypes> = BTreeMap<T::ChainBlockNumber, Vec<T::Event>>;

		type Multiset<A> = Container<BTreeSet<A>>;

		// let active_events = |s: &BlocksData<T>| -> BTreeSet<(T::ChainBlockNumber, T::Event)> {
		let active_events = |s: &BlocksData<T>| -> Multiset<T::Event> {
			s.iter()
				.flat_map(|(height, (data, age))| {
					let mut x: BlockProcessor<T> = Default::default();
					x.rules.run((*height, (0..*age), data.clone()))
				})
				.map(|(number, event)| event)
				.collect()
		};

		let stored_events =
			|s: &ReorgData<T>| -> Multiset<T::Event> { s.values().flatten().cloned().collect() };

		let events =
			|s: &Self::State| active_events(&s.blocks_data) + stored_events(&s.reorg_events);
		let deleted_events = active_events(&output.deleted_data) +
			stored_events(&output.deleted_events.iter().cloned().collect());

		let executed_events =
			|| -> Multiset<_> { output.events.iter().map(|(k, v)| v).cloned().collect() };
		let executed_events_vector = || -> Container<BTreeMultiSet<_>> {
			output.events.iter().map(|(k, v)| v).cloned().collect()
		};

		let latest_block = match input {
			SMBlockProcessorInput::NewBlockData(n, _, _) => n,
			SMBlockProcessorInput::ChainProgress(ChainProgressInner::Progress(x)) => x,
			SMBlockProcessorInput::ChainProgress(ChainProgressInner::Reorg(x)) => x.end(),
		};
		let reorg =
			matches!(input, SMBlockProcessorInput::ChainProgress(ChainProgressInner::Reorg(_)));
		let blocks =
			|d: &BlocksData<T>| d.values().map(|(d, age)| d.clone()).collect::<BTreeSet<_>>();

		let deleted_new: BTreeSet<T::BlockData> = match input {
			SMBlockProcessorInput::NewBlockData(n, i, x) if i.saturating_forward(3) <= *n =>
				BTreeSet::from([x.clone()]),
			_ => BTreeSet::new(),
		};

		let new_block: BTreeSet<T::BlockData> = match input {
			SMBlockProcessorInput::NewBlockData(n, i, x) => BTreeSet::from([x.clone()]),
			_ => BTreeSet::new(),
		};

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
			in executed_events_vector().0.0.iter().all(|(x, n)| *n == 1);

			// TODO: handle the reorg case
			"blocks either stay in the blockstore or are included in the 'deleted output'"
			in if !reorg {
				blocks(&pre.blocks_data).is_subset(&blocks(&post.blocks_data).merge(blocks(&output.deleted_data)))
			} else {true};

			"new blocks are added to block data or are immediately deleted"
			in new_block.is_subset(&blocks(&post.blocks_data).merge(deleted_new));
		}
	}
}
