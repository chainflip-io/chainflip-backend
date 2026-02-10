use crate::{
	chainflip::witnessing::{
		elections::TypesFor,
		ethereum_elections::{BlockDataScUtils, EthereumScUtilsWitnessing, ScUtilsCall},
	},
	Runtime,
};
use cf_chains::{Chain, Ethereum};
use cf_traits::Hook;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_elections::electoral_systems::block_witnesser::state_machine::{
	ExecuteHook, HookTypeFor, RulesHook,
};
use sp_std::{vec, vec::Vec};

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Deserialize,
	Serialize,
	Ord,
	PartialOrd,
)]
pub enum EthEvent<T> {
	Witness(T),
}

type TypesScUtilsWitnessing = TypesFor<EthereumScUtilsWitnessing>;
type BlockNumber = <Ethereum as Chain>::ChainBlockNumber;

impl Hook<HookTypeFor<TypesScUtilsWitnessing, ExecuteHook>> for TypesScUtilsWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<ScUtilsCall>)>) {
		for (_, event) in events {
			match event {
				EthEvent::Witness(call) => {
					if let Err(err) = pallet_cf_funding::Pallet::<Runtime>::execute_sc_call(
						pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
						call.deposit_and_call.clone(),
						call.caller,
						// use 0 padded ethereum address as account_id which the flip funds
						// are associated with on SC
						call.caller_account_id,
						call.eth_tx_hash,
					) {
						log::error!(
							"Failed to execute Ethereum sc call {:?}: Error: {:?}",
							call.deposit_and_call.call,
							err
						)
					}
				},
			};
		}
	}
}

#[macro_export]
macro_rules! impl_rules_hook {
	($types:ty, $block_data:ty, $event:ty) => {
		impl Hook<HookTypeFor<$types, RulesHook>> for $types {
			fn run(
				&mut self,
				(age, block_data, safety_margin): (Range<u32>, $block_data, u32),
			) -> Vec<$event> {
				if age.contains(&safety_margin) {
					block_data.into_iter().map(<$event>::Witness).collect()
				} else {
					vec![]
				}
			}
		}
	};
}

impl_rules_hook!(TypesScUtilsWitnessing, BlockDataScUtils, EthEvent<ScUtilsCall>);
