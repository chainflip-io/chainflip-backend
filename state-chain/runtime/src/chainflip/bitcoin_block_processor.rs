use crate::{
	chainflip::{
		bitcoin_elections::{BitcoinEgressWitnessing, EgressBlockData},
		elections::TypesFor,
	},
	BitcoinBroadcaster, Runtime,
};
use cf_chains::{btc::BlockNumber, instances::BitcoinInstance};
use cf_traits::Hook;
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::electoral_systems::block_witnesser::state_machine::{
	ExecuteHook, HookTypeFor, RulesHook,
};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum BtcEvent<T> {
	PreWitness(T),
	Witness(T),
}

impl<T> BtcEvent<T> {
	fn inner_witness(&self) -> &T {
		match self {
			BtcEvent::PreWitness(w) | BtcEvent::Witness(w) => w,
		}
	}
}

type TypesEgressWitnessing = TypesFor<BitcoinEgressWitnessing>;

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep only the `Witness` variant,
fn dedup_events<T: Ord + Clone>(
	events: Vec<(BlockNumber, BtcEvent<T>)>,
) -> Vec<(BlockNumber, BtcEvent<T>)> {
	let mut chosen: BTreeMap<T, (BlockNumber, BtcEvent<T>)> = BTreeMap::new();

	for (block, event) in events {
		let witness = event.inner_witness().clone();

		// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
		if !chosen.contains_key(&witness) ||
			(matches!(chosen.get(&witness), Some((_, BtcEvent::PreWitness(_)))) &&
				matches!(event, BtcEvent::Witness(_)))
		{
			chosen.insert(witness, (block, event));
		}
	}

	chosen.into_values().collect()
}
impl Hook<HookTypeFor<TypesEgressWitnessing, ExecuteHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		events: Vec<(BlockNumber, BtcEvent<TransactionConfirmation<Runtime, BitcoinInstance>>)>,
	) {
		let deduped_events = dedup_events(events);
		for (_, event) in &deduped_events {
			match event {
				BtcEvent::PreWitness(_) => { /* We don't care about pre-witnessing an egress*/ },
				BtcEvent::Witness(egress) => {
					#[expect(clippy::unit_arg)]
					if let Err(err) = BitcoinBroadcaster::egress_success(
						pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
						egress.tx_out_id,
						egress.signer_id.clone(),
						egress.tx_fee,
						egress.tx_metadata,
						egress.transaction_ref,
					) {
						log::error!(
							"Failed to execute Bitcoin egress success: TxOutId: {:?}, Error: {:?}",
							egress.tx_out_id,
							err
						)
					}
				},
			}
		}
	}
}

impl Hook<HookTypeFor<TypesEgressWitnessing, RulesHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, EgressBlockData, u32),
	) -> Vec<BtcEvent<TransactionConfirmation<Runtime, BitcoinInstance>>> {
		if age.contains(&safety_margin) {
			return block_data
				.iter()
				.map(|egress_witness| BtcEvent::Witness(egress_witness.clone()))
				.collect::<Vec<_>>();
		}
		vec![]
	}
}

#[cfg(test)]
mod tests {
	use crate::chainflip::bitcoin_block_processor::{dedup_events, BtcEvent};

	#[test]
	fn dedup_events_test() {
		let events = vec![
			(10, BtcEvent::<u8>::Witness(9)),
			(8, BtcEvent::<u8>::PreWitness(9)),
			(10, BtcEvent::<u8>::Witness(10)),
			(10, BtcEvent::<u8>::Witness(11)),
			(8, BtcEvent::<u8>::PreWitness(11)),
			(10, BtcEvent::<u8>::PreWitness(12)),
		];
		let deduped_events = dedup_events(events);

		assert_eq!(
			deduped_events,
			vec![
				(10, BtcEvent::<u8>::Witness(9)),
				(10, BtcEvent::<u8>::Witness(10)),
				(10, BtcEvent::<u8>::Witness(11)),
				(10, BtcEvent::<u8>::PreWitness(12)),
			]
		)
	}
}
