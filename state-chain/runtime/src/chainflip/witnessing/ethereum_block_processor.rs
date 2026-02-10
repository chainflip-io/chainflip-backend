use crate::{
	chainflip::witnessing::{
		elections::TypesFor,
		ethereum_elections::{
			BlockDataKeyManager, BlockDataScUtils, EthereumKeyManagerEvent,
			EthereumKeyManagerWitnessing, EthereumScUtilsWitnessing, ScUtilsCall,
		},
	},
	EthereumBroadcaster, Runtime,
};
use cf_chains::{instances::EthereumInstance, Chain, Ethereum};
use cf_traits::Hook;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_broadcast::TransactionConfirmation;
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

type TypesKeyManagerWitnessing = TypesFor<EthereumKeyManagerWitnessing>;
type TypesScUtilsWitnessing = TypesFor<EthereumScUtilsWitnessing>;
type BlockNumber = <Ethereum as Chain>::ChainBlockNumber;

impl Hook<HookTypeFor<TypesKeyManagerWitnessing, ExecuteHook>> for TypesKeyManagerWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<EthereumKeyManagerEvent>)>) {
		for (block_number, event) in events {
			match event {
				EthEvent::Witness(call) => {
					match call {
						EthereumKeyManagerEvent::AggKeySetByGovKey {
							new_public_key,
							block_number,
							tx_id: _,
						} => {
							pallet_cf_vaults::Pallet::<Runtime, EthereumInstance>::inner_vault_key_rotated_externally(new_public_key, block_number);
						},
						EthereumKeyManagerEvent::SignatureAccepted {
							tx_out_id,
							signer_id,
							tx_fee,
							tx_metadata,
							transaction_ref,
						} => {
							if let Err(err) = EthereumBroadcaster::egress_success(
								pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
								TransactionConfirmation {
									tx_out_id,
									signer_id,
									tx_fee,
									tx_metadata,
									transaction_ref,
								},
								block_number,
							) {
								log::error!(
									"Failed to execute Ethereum egress success: TxOutId: {:?}, Error: {:?}",
									tx_out_id,
									err
								)
							}
						},
						EthereumKeyManagerEvent::GovernanceAction { call_hash } => {
							if let Err(err) =
								pallet_cf_governance::Pallet::<Runtime>::set_whitelisted_call_hash(
									pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold
										.into(),
									call_hash,
								) {
								log::error!(
									"Failed to whitelist Ethereum governance call hash: {:?}, Error: {:?}",
									call_hash,
									err
								);
							}
						},
					};
				},
			};
		}
	}
}

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

impl_rules_hook!(TypesKeyManagerWitnessing, BlockDataKeyManager, EthEvent<EthereumKeyManagerEvent>);
impl_rules_hook!(TypesScUtilsWitnessing, BlockDataScUtils, EthEvent<ScUtilsCall>);
