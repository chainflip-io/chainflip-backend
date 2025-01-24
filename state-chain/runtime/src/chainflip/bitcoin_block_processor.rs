use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::{
	chainflip::bitcoin_elections::BlockData, BitcoinChainTracking, BitcoinIngressEgress, Block,
	ConstU32, Runtime,
};
use cf_chains::{btc, btc::BlockNumber, instances::BitcoinInstance};
use cf_primitives::chains::Bitcoin;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::TypeInfo, BoundedVec, Deserialize, Serialize};
use log::warn;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::{
		primitives::ChainProgressInner,
		state_machine::{BWProcessorTypes, BlockWitnesserProcessor},
	},
	state_machine::{
		core::{Hook, IndexOf, Indexed, Validate},
		state_machine::StateMachine,
	},
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum BtcEvent {
	PreWitness(BlockNumber, DepositWitness<Bitcoin>),
	Witness(BlockNumber, DepositWitness<Bitcoin>),
}
impl BtcEvent {
	pub fn deposit_witness(&self) -> &DepositWitness<Bitcoin> {
		match self {
			BtcEvent::PreWitness(_, dw) | BtcEvent::Witness(_, dw) => dw,
		}
	}
	pub fn equal_inner(&self, other: &BtcEvent) -> bool {
		self.deposit_witness() == other.deposit_witness()
	}
}

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep the `Witness` variant,
/// - otherwise keep the single `PreWitness`.
pub fn deduplicate_btc_events(events: Vec<BtcEvent>) -> Vec<BtcEvent> {
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

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct BlockWitnessingProcessorDefinition {}

impl BWProcessorTypes for BlockWitnessingProcessorDefinition {
	type ChainBlockNumber = BlockNumber;
	type BlockData = BlockData;
	type Event = BtcEvent;
	type Rules = ApplyRulesHook;
	type Execute = ExecuteEventHook;
}

#[derive(
	Clone, Debug, Eq, PartialEq, Encode, Decode, TypeInfo, MaxEncodedLen, Serialize, Deserialize,
)]
pub struct DepositChannelWitnessingProcessor<T: BWProcessorTypes> {
	pub blocks_data: BTreeMap<T::ChainBlockNumber, (T::BlockData, T::ChainBlockNumber)>,
	pub reorg_events: BTreeMap<T::ChainBlockNumber, Vec<T::Event>>,
	pub rules: T::Rules,
	pub execute: T::Execute,
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
		}
	}
}

impl BlockWitnesserProcessor<BlockWitnessingProcessorDefinition>
	for DepositChannelWitnessingProcessor<BlockWitnessingProcessorDefinition>
{
	fn process_block_data(
		&mut self,
		chain_progress: ChainProgressInner<BlockNumber>,
	) -> Vec<BtcEvent> {
		let last_block: BlockNumber;
		match chain_progress {
			ChainProgressInner::Progress(last_height) => {
				last_block = last_height;
			},
			ChainProgressInner::Reorg(range) => {
				last_block = *range.end();
				for n in range.clone() {
					let block_data = self.blocks_data.remove(&n);
					if let Some((data, last_age)) = block_data {
						// We need to get only events already processed (last_age not included since
						// that value has still to be processed)
						for age in 0..last_age {
							self.reorg_events
								.insert(n, self.process_rules_for_age_and_block(n, age, &data));
						}
					}
				}
			},
		}
		let last_events = deduplicate_btc_events(self.process_rules(last_block));
		for event in &last_events {
			self.execute.run(event.clone());
		}
		self.clean_old(last_block);
		last_events
	}

	fn insert(&mut self, n: BlockNumber, block_data: BlockData) {
		self.blocks_data.insert(n, (block_data, 0));
	}

	fn clean_old(&mut self, n: BlockNumber) {
		self.blocks_data.retain(|key, (data, age)| {
			*age <= BitcoinIngressEgress::witness_safety_margin().unwrap()
		});
		self.reorg_events
			.retain(|key, data| *key > n - crate::chainflip::bitcoin_elections::BUFFER_EVENTS);
	}

	fn process_rules(&mut self, last_height: BlockNumber) -> Vec<BtcEvent> {
		warn!("Processing rules... last_height: {last_height:#?}");
		let mut last_events: Vec<BtcEvent> = vec![];

		for (block, (data, next_age)) in self.blocks_data.clone() {
			warn!("Rules for block {block:?}, next_age: {next_age:?}, data: {data:?}");
			for age in next_age..=last_height - block {
				last_events = last_events
					.into_iter()
					.chain(self.process_rules_for_age_and_block(block, age, &data))
					.collect();
			}
			//Updating the age of the block, this can problably done in another way by mutably
			// looping through the map
			self.blocks_data.insert(block, (data.clone(), last_height - block + 1));
		}
		warn!("Rules produced these events: {last_events:#?}");

		last_events
	}

	fn process_rules_for_age_and_block(
		&self,
		block: BlockNumber,
		age: BlockNumber,
		data: &crate::chainflip::bitcoin_elections::BlockData,
	) -> Vec<BtcEvent> {
		let mut events: Vec<BtcEvent> = vec![];
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
	fn run(&self, input: BtcEvent) -> () {
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
		&self,
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
		if age == BitcoinIngressEgress::witness_safety_margin().unwrap() {
			return block_data
				.iter()
				.map(|deposit_witness| BtcEvent::Witness(block, deposit_witness.clone()))
				.collect::<Vec<BtcEvent>>();
		}
		vec![]
	}
}

/// State-Machine Block Witness Processor
#[derive(Clone, Debug)]
pub enum SMBlockProcessorInput {
	NewBlockData(BlockNumber, BlockNumber, crate::chainflip::bitcoin_elections::BlockData),
	ChainProgress(ChainProgressInner<BlockNumber>),
}

impl Indexed for SMBlockProcessorInput {
	type Index = ();
	fn has_index(&self, _idx: &Self::Index) -> bool {
		true
	}
}
impl Validate for SMBlockProcessorInput {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

pub type SMBlockProcessorState =
	DepositChannelWitnessingProcessor<BlockWitnessingProcessorDefinition>;
impl Validate for SMBlockProcessorState {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
pub struct SMBlockProcessorOutput(Vec<BtcEvent>);
impl Validate for SMBlockProcessorOutput {
	type Error = ();
	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}
pub struct SMBlockProcessor;

impl StateMachine for SMBlockProcessor {
	type Input = SMBlockProcessorInput;
	type Settings = ();
	type Output = SMBlockProcessorOutput;
	type State = SMBlockProcessorState;

	fn input_index(s: &Self::State) -> IndexOf<Self::Input> {
		()
	}

	fn step(s: &mut Self::State, i: Self::Input, _set: &Self::Settings) -> Self::Output {
		match i {
			SMBlockProcessorInput::NewBlockData(last_height, n, deposits) => {
				s.insert(n, deposits);
				SMBlockProcessorOutput(
					s.process_block_data(ChainProgressInner::Progress(last_height)),
				)
			},
			SMBlockProcessorInput::ChainProgress(inner) =>
				SMBlockProcessorOutput(s.process_block_data(inner)),
		}
	}
}
