use crate::{
	chainflip::bitcoin_elections::{
		BitcoinEgressWitnessing, BitcoinVaultDepositWitnessing, BlockDataDepositChannel,
		BlockDataVaultDeposit, EgressBlockData,
	},
	BitcoinBroadcaster, BitcoinIngressEgress, Runtime,
};
use cf_chains::{btc::BlockNumber, instances::BitcoinInstance};
use cf_primitives::chains::Bitcoin;
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::state_machine::{
		AnyEvent, ExecuteHook, HookTypeFor, RulesHook, SafetyMarginHook,
	},
	state_machine::core::Hook,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use sp_std::{collections::btree_map::BTreeMap, iter::Step, vec, vec::Vec};

use super::{bitcoin_elections::BitcoinDepositChannelWitnessing, elections::TypesFor};

type TypesDepositChannelWitnessing = TypesFor<BitcoinDepositChannelWitnessing>;
type TypesVaultDepositWitnessing = TypesFor<BitcoinVaultDepositWitnessing>;

type TypesEgressWitnessing = TypesFor<BitcoinEgressWitnessing>;

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep only the `Witness` variant,
fn dedup_events<T: Ord + Clone>(
	events: Vec<(BlockNumber, AnyEvent<T>)>,
) -> Vec<(BlockNumber, AnyEvent<T>)> {
	let mut chosen: BTreeMap<T, (BlockNumber, AnyEvent<T>)> = BTreeMap::new();

	for (block, event) in events {
		let deposit = event.inner().clone();

		// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
		if !chosen.contains_key(&deposit) ||
			(matches!(chosen.get(&deposit), Some((_, AnyEvent::PreWitness(_)))) &&
				matches!(event, AnyEvent::Witness(_)))
		{
			chosen.insert(deposit, (block, event));
		}
	}

	chosen.into_values().collect()
}
impl Hook<HookTypeFor<TypesDepositChannelWitnessing, ExecuteHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, AnyEvent<DepositWitness<Bitcoin>>)>) {
		let deduped_events = dedup_events(events);
		for (block, event) in &deduped_events {
			match event {
				AnyEvent::PreWitness(deposit) => {
					let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(
						deposit.clone(),
						*block,
					);
				},
				AnyEvent::Witness(deposit) => {
					BitcoinIngressEgress::process_channel_deposit_full_witness(
						deposit.clone(),
						*block,
					);
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(
		&mut self,
		events: Vec<(BlockNumber, AnyEvent<VaultDepositWitness<Runtime, BitcoinInstance>>)>,
	) {
		let deduped_events = dedup_events(events);

		for (block, event) in &deduped_events {
			match event {
				AnyEvent::PreWitness(deposit) => {
					BitcoinIngressEgress::process_vault_swap_request_prewitness(
						*block,
						deposit.clone(),
					);
				},
				AnyEvent::Witness(deposit) => {
					BitcoinIngressEgress::process_vault_swap_request_full_witness(
						*block,
						deposit.clone(),
					);
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesEgressWitnessing, ExecuteHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		events: Vec<(BlockNumber, AnyEvent<TransactionConfirmation<Runtime, BitcoinInstance>>)>,
	) {
		let deduped_events = dedup_events(events);
		for (_, event) in &deduped_events {
			match event {
				AnyEvent::PreWitness(_) => { /* We don't care about pre-witnessing an egress*/ },
				AnyEvent::Witness(egress) => {
					BitcoinBroadcaster::broadcast_success(egress.clone());
				},
			}
		}
	}
}

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, RulesHook>> for TypesDepositChannelWitnessing {
	fn run(
		&mut self,
		(block, age, block_data): (BlockNumber, Range<u32>, BlockDataDepositChannel),
	) -> Vec<(BlockNumber, AnyEvent<DepositWitness<Bitcoin>>)> {
		let mut results: Vec<(BlockNumber, AnyEvent<DepositWitness<Bitcoin>>)> = vec![];
		if age.contains(&0u32) {
			results.extend(
				block_data
					.iter()
					.map(|deposit_witness| (block, AnyEvent::PreWitness(deposit_witness.clone())))
					.collect::<Vec<_>>(),
			)
		}
		if age.contains(
			&(u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0
				as u32),
		) {
			results.extend(
				block_data
					.iter()
					.map(|deposit_witness| (block, AnyEvent::Witness(deposit_witness.clone())))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

impl Hook<HookTypeFor<TypesVaultDepositWitnessing, RulesHook>> for TypesVaultDepositWitnessing {
	fn run(
		&mut self,
		(block, age, block_data): (BlockNumber, Range<u32>, BlockDataVaultDeposit),
	) -> Vec<(BlockNumber, AnyEvent<VaultDepositWitness<Runtime, BitcoinInstance>>)> {
		let mut results: Vec<(
			BlockNumber,
			AnyEvent<VaultDepositWitness<Runtime, BitcoinInstance>>,
		)> = vec![];
		if age.contains(&0u32) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| (block, AnyEvent::PreWitness(vault_deposit.clone())))
					.collect::<Vec<_>>(),
			)
		}
		if age.contains(
			&(u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0
				as u32),
		) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| (block, AnyEvent::Witness(vault_deposit.clone())))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

impl Hook<HookTypeFor<TypesEgressWitnessing, RulesHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		(block, age, block_data): (BlockNumber, Range<u32>, EgressBlockData),
	) -> Vec<(BlockNumber, AnyEvent<TransactionConfirmation<Runtime, BitcoinInstance>>)> {
		if age.contains(
			&(u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0
				as u32),
		) {
			return block_data
				.iter()
				.map(|egress_witness| (block, AnyEvent::Witness(egress_witness.clone())))
				.collect::<Vec<_>>();
		}
		vec![]
	}
}

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, SafetyMarginHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, _input: ()) -> u32 {
		u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0 as u32
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, SafetyMarginHook>>
	for TypesVaultDepositWitnessing
{
	fn run(&mut self, _input: ()) -> u32 {
		u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0 as u32
	}
}

impl Hook<HookTypeFor<TypesEgressWitnessing, SafetyMarginHook>> for TypesEgressWitnessing {
	fn run(&mut self, _input: ()) -> u32 {
		u64::steps_between(&0, &BitcoinIngressEgress::witness_safety_margin().unwrap_or(0)).0 as u32
	}
}

#[cfg(test)]
mod tests {
	use crate::chainflip::bitcoin_block_processor::{dedup_events, AnyEvent};

	#[test]
	fn dedup_events_test() {
		let events = vec![
			(10, AnyEvent::<u8>::Witness(9)),
			(8, AnyEvent::<u8>::PreWitness(9)),
			(10, AnyEvent::<u8>::Witness(10)),
			(10, AnyEvent::<u8>::Witness(11)),
			(8, AnyEvent::<u8>::PreWitness(11)),
			(10, AnyEvent::<u8>::PreWitness(12)),
		];
		let deduped_events = dedup_events(events);
		assert_eq!(deduped_events.len(), 4);
		assert!(!deduped_events.contains(&(8, AnyEvent::<u8>::PreWitness(9))));
		assert!(!deduped_events.contains(&(8, AnyEvent::<u8>::PreWitness(11))));
	}
	/*
	   use cf_chains::btc::BlockNumber;
	   use std::collections::BTreeMap;

	   // use crate::chainflip::bitcoin_block_processor::{ApplyRulesHook, SafetyMarginHook};
	   use codec::{Decode, Encode};
	   use core::ops::RangeInclusive;
	   use frame_support::pallet_prelude::TypeInfo;
	   use pallet_cf_elections::electoral_systems::block_witnesser::primitives::ChainProgressInner;

	   use crate::chainflip::bitcoin_block_processor::DedupEventsHook;
	   use cf_chains::btc::BtcAmount;
	   use pallet_cf_elections::electoral_systems::{
		   block_witnesser::{
			   block_processor::{BlockProcessor},
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
	) -> BoxedStrategy<BTreeMap<BlockNumber, (MockBlockData, u32)>> {
		prop::collection::btree_map(
			0..number_of_blocks,
			(vec![block_data()], (0..=0u32)),
			RangeInclusive::new(0, number_of_blocks as usize),
		)
		.boxed()
	}
	#[allow(dead_code)]
	fn generate_state() -> BoxedStrategy<BlockProcessor<MockBlockProcessorDefinition>> {
		blocks_data(10)
			.prop_map(|data| BlockProcessor {
				blocks_data: data,
				reorg_events: Default::default(),
				rules: ApplyRulesHook {},
				execute: IncreasingHook::<(BlockNumber, MockBtcEvent), ()>::default(),
				dedup_events: DedupEventsHook {},
				safety_margin: SafetyMarginHook {},
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
		   PreWitness(MockDeposit),
		   Witness(MockDeposit),
	   }
	   impl MockBtcEvent {
		   pub fn deposit_witness(&self) -> &MockDeposit {
			   match self {
				   MockBtcEvent::PreWitness(dw) | MockBtcEvent::Witness(dw) => dw,
			   }
		   }
	   }

	   impl Hook<(BlockNumber, u32, MockBlockData), Vec<(BlockNumber, MockBtcEvent)>> for ApplyRulesHook {
		   fn run(
			   &mut self,
			   (block, age, block_data): (BlockNumber, u32, MockBlockData),
		   ) -> Vec<(BlockNumber, MockBtcEvent)> {
			   // Prewitness rule
			   if age == 0 {
				   return block_data
					   .iter()
					   .map(|deposit_witness| {
						   (block, MockBtcEvent::PreWitness(deposit_witness.clone()))
					   })
					   .collect::<Vec<(BlockNumber, MockBtcEvent)>>();
			   }
			   //Full witness rule
			   if age == 3 {
				   return block_data
					   .iter()
					   .map(|deposit_witness| (block, MockBtcEvent::Witness(deposit_witness.clone())))
					   .collect::<Vec<(BlockNumber, MockBtcEvent)>>();
			   }
			   vec![]
		   }
	   }

	   impl Hook<Vec<(BlockNumber, MockBtcEvent)>, Vec<(BlockNumber, MockBtcEvent)>> for DedupEventsHook {
		   fn run(
			   &mut self,
			   events: Vec<(BlockNumber, MockBtcEvent)>,
		   ) -> Vec<(BlockNumber, MockBtcEvent)> {
			   // Map: deposit_witness -> chosen BtcEvent
			   // todo! this is annoying, it require us to implement Ord down to the Chain type
			   let mut chosen: BTreeMap<MockDeposit, (BlockNumber, MockBtcEvent)> = BTreeMap::new();

			   for (block, event) in events {
				   let deposit = event.deposit_witness();

				   match chosen.get(deposit) {
					   None => {
						   // No event yet for this deposit, store it
						   chosen.insert(deposit.clone(), (block, event));
					   },
					   Some((_, existing_event)) => {
						   // There's already an event for this deposit
						   match (existing_event, &event) {
							   // If we already have a Witness, do nothing
							   (MockBtcEvent::Witness(_), MockBtcEvent::PreWitness(_)) => (),
							   // If we have a PreWitness and the new event is a Witness, override it
							   (MockBtcEvent::PreWitness(_), MockBtcEvent::Witness(_)) => {
								   chosen.insert(deposit.clone(), (block, event));
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
	   impl BWProcessorTypes for MockBlockProcessorDefinition {
		   type ChainBlockNumber = BlockNumber;
		   type BlockData = MockBlockData;
		   type Event = MockBtcEvent;
		   type Rules = ApplyRulesHook;
		   type Execute = IncreasingHook<(Self::ChainBlockNumber, Self::Event), ()>;
		   type DedupEvents = DedupEventsHook;
		   type SafetyMargin = SafetyMarginHook;
	   }

	   #[test]
	   fn test() {
		   let _processor = BlockProcessor::<MockBlockProcessorDefinition>::default();
	   }
	*/
}
