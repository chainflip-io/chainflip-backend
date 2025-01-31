use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

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

// State-Machine Block Witness Processor
// #[derive(Clone, Debug)]
// #[allow(dead_code)]
// pub enum SMBlockProcessorInput {
// 	NewBlockData(BlockNumber, BlockNumber, BlockData),
// 	ChainProgress(ChainProgressInner<BlockNumber>),
// }
//
// impl Indexed for SMBlockProcessorInput {
// 	type Index = ();
// 	fn has_index(&self, _idx: &Self::Index) -> bool {
// 		true
// 	}
// }
// impl Validate for SMBlockProcessorInput {
// 	type Error = ();
//
// 	fn is_valid(&self) -> Result<(), Self::Error> {
// 		Ok(())
// 	}
// }
//
// pub type SMBlockProcessorState =
// 	DepositChannelWitnessingProcessor<BlockWitnessingProcessorDefinition>;
// impl Validate for SMBlockProcessorState {
// 	type Error = ();
// 	fn is_valid(&self) -> Result<(), Self::Error> {
// 		Ok(())
// 	}
// }
// pub struct SMBlockProcessorOutput(Vec<BtcEvent>);
// impl Validate for SMBlockProcessorOutput {
// 	type Error = ();
// 	fn is_valid(&self) -> Result<(), Self::Error> {
// 		Ok(())
// 	}
// }
// pub struct SMBlockProcessor;
//
// impl StateMachine for SMBlockProcessor {
// 	type Input = SMBlockProcessorInput;
// 	type Settings = ();
// 	type Output = SMBlockProcessorOutput;
// 	type State = SMBlockProcessorState;
//
// 	fn input_index(_s: &Self::State) -> IndexOf<Self::Input> {}
//
// 	fn step(s: &mut Self::State, i: Self::Input, _set: &Self::Settings) -> Self::Output {
// 		match i {
// 			SMBlockProcessorInput::NewBlockData(last_height, n, deposits) => {
// 				s.insert(n, deposits);
// 				SMBlockProcessorOutput(
// 					s.process_block_data(ChainProgressInner::Progress(last_height)),
// 				)
// 			},
// 			SMBlockProcessorInput::ChainProgress(inner) =>
// 				SMBlockProcessorOutput(s.process_block_data(inner)),
// 		}
// 	}
//
// 	// #[cfg(test)]
// 	// fn step_specification(
// 	// 	before: &Self::State,
// 	// 	input: &Self::Input,
// 	// 	_settings: &Self::Settings,
// 	// 	after: &Self::State,
// 	// ) {
// 	// 	assert!(
// 	// 		after.blocks_data.len() <=
// 	// 			BitcoinIngressEgress::witness_safety_margin().unwrap() as usize,
// 	// 		"Too many blocks data, we should never have more than safety margin blocks"
// 	// 	);
// 	//
// 	// 	match input {
// 	// 		SMBlockProcessorInput::ChainProgress(chain_progress) => match chain_progress {
// 	// 			ChainProgressInner::Progress(_last_height) => {
// 	// 				assert!(after.reorg_events.len() <= before.reorg_events.len(), "If no reorg happened,
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
// }
//
// #[cfg(test)]
// mod tests {
// 	use cf_chains::{
// 		btc::{BlockNumber, Utxo, UtxoId},
// 		Bitcoin, Chain,
// 	};
// 	use std::collections::BTreeMap;
//
// 	use crate::chainflip::{
// 		bitcoin_block_processor::{
// 			ApplyRulesHook, ExecuteEventHook,
// 		},
// 		bitcoin_elections::BlockData,
// 	};
// 	use core::ops::RangeInclusive;
// 	use codec::{Decode, Encode};
// 	use frame_support::pallet_prelude::TypeInfo;
// 	use log::warn;
// 	use pallet_cf_elections::electoral_systems::{
// 		block_witnesser::primitives::ChainProgressInner,
// 		state_machine::state_machine2::StateMachine,
// 	};
// 	use pallet_cf_ingress_egress::DepositWitness;
// 	use proptest::{
// 		prelude::{any, prop, BoxedStrategy, Strategy},
// 		prop_oneof,
// 	};
// 	use serde::{Deserialize, Serialize};
// 	use cf_chains::btc::BtcAmount;
// 	use cf_chains::instances::BitcoinInstance;
// 	use pallet_cf_elections::electoral_systems::block_witnesser::state_machine::BWProcessorTypes;
// 	use pallet_cf_elections::electoral_systems::state_machine::core::Hook;
// 	// use pallet_cf_elections::electoral_systems::state_machine::core::hook_test_utils::IncreasingHook;
// 	use crate::{BitcoinIngressEgress,
// chainflip::bitcoin_block_processor::{BlockWitnessingProcessorDefinition, BtcEvent}}; 	// use
// pallet_cf_elections::electoral_systems::state_machine::core::hook_test_utils::IncreasingHook;
//
// 	use pallet_cf_elections::electoral_systems::block_witnesser::block_processor::DepositChannelWitnessingProcessor;
//
// 	fn block_data() -> BoxedStrategy<DepositWitness<Bitcoin>> {
// 		(any::<u64>(), any::<u32>())
// 			.prop_map(|(amount, numb)| DepositWitness {
// 				deposit_address: <Bitcoin as Chain>::ChainAccount::Taproot([0; 32]),
// 				asset: <Bitcoin as Chain>::ChainAsset::Btc,
// 				amount: amount.clone(),
// 				deposit_details: Utxo {
// 					id: UtxoId { tx_id: Default::default(), vout: numb },
// 					amount,
// 					deposit_address: cf_chains::btc::deposit_address::DepositAddress {
// 						pubkey_x: [0; 32],
// 						script_path: None,
// 					},
// 				},
// 			})
// 			.boxed()
// 	}
//
// 	fn blocks_data(
// 		number_of_blocks: u64,
// 	) -> BoxedStrategy<BTreeMap<BlockNumber, (BlockData, BlockNumber)>> {
// 		prop::collection::btree_map(
// 			0..number_of_blocks,
// 			(vec![block_data()], (0..=0u64)),
// 			RangeInclusive::new(0, number_of_blocks as usize),
// 		)
// 		.boxed()
// 	}
//
// 	fn generate_state() -> BoxedStrategy<SMBlockProcessorState> {
// 		blocks_data(10)
// 			.prop_map(|data| SMBlockProcessorState {
// 				blocks_data: data,
// 				reorg_events: Default::default(),
// 				rules: ApplyRulesHook {},
// 				execute: ExecuteEventHook {},
// 			})
// 			.boxed()
// 	}
//
// 	fn generate_input() -> BoxedStrategy<SMBlockProcessorInput> {
// 		prop_oneof![
// 			(any::<u64>(), block_data()).prop_map(|(n, data)| SMBlockProcessorInput::NewBlockData(
// 				n,
// 				n,
// 				vec![data]
// 			)),
// 			prop_oneof![
// 				(0..=5u64).prop_map(|n| ChainProgressInner::Progress(n)),
// 				(0..=5u64).prop_map(|n| ChainProgressInner::Reorg(
// 					RangeInclusive::<BlockNumber>::new(n, n + 2)
// 				)),
// 			]
// 			.prop_map(|inner| SMBlockProcessorInput::ChainProgress(inner)),
// 		]
// 		.boxed()
// 	}
//
// 	// #[test]
// 	// fn main_test() {
// 	// 	<SMBlockProcessor as StateMachine>::test(file!(), generate_state(), (), generate_input());
// 	// }
//
// 	struct MockBlockProcessorDefinition {}
// 	#[derive(
// 		Clone,
// 		PartialEq,
// 		Eq,
// 		Serialize,
// 		Deserialize,
// 		Encode,
// 		TypeInfo,
// 		Decode,
// 		Debug,
// 		Ord,
// 		PartialOrd,
// 		Default
// 	)]
// 	struct MockDeposit {
// 		pub amount: BtcAmount,
// 		pub deposit_address: String,
// 	}
// 	type  MockBlockData = Vec<MockDeposit>;
//
// 	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
// 	enum MockBtcEvent {
// 		PreWitness(BlockNumber, MockDeposit),
// 		Witness(BlockNumber, MockDeposit),
// 	}
// 	impl MockBtcEvent {
// 		pub fn deposit_witness(&self) -> &MockDeposit {
// 			match self {
// 				MockBtcEvent::PreWitness(_, dw) | MockBtcEvent::Witness(_, dw) => dw,
// 			}
// 		}
// 		pub fn equal_inner(&self, other: MockBtcEvent) -> bool {
// 			self.deposit_witness() == other.deposit_witness()
// 		}
// 	}
//
// 	impl Hook<(BlockNumber, BlockNumber, MockBlockData), Vec<MockBtcEvent>> for ApplyRulesHook {
// 		fn run(
// 			&self,
// 			(block, age, block_data): (BlockNumber, BlockNumber, MockBlockData),
// 		) -> Vec<MockBtcEvent> {
// 			// Prewitness rule
// 			if age == 0 {
// 				return block_data
// 					.iter()
// 					.map(|deposit_witness| MockBtcEvent::PreWitness(block, deposit_witness.clone()))
// 					.collect::<Vec<MockBtcEvent>>();
// 			}
// 			//Full witness rule
// 			if age == BitcoinIngressEgress::witness_safety_margin().unwrap() {
// 				return block_data
// 					.iter()
// 					.map(|deposit_witness| MockBtcEvent::Witness(block, deposit_witness.clone()))
// 					.collect::<Vec<MockBtcEvent>>();
// 			}
// 			vec![]
// 		}
// 	}
// 	impl BWProcessorTypes for MockBlockProcessorDefinition {
// 		type ChainBlockNumber = BlockNumber;
// 		type BlockData = MockBlockData;
// 		type Event = MockBtcEvent;
// 		type Rules = ApplyRulesHook;
// 		type Execute = IncreasingHook<MockBtcEvent,()>;
// 	}
//
// 	#[test]
// 	fn test() {
// 		let mut processor =
// DepositChannelWitnessingProcessor::<MockBlockProcessorDefinition>::default();
//
// 	}
// }
