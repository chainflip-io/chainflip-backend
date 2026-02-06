use crate::{
	chainflip::witnessing::{
		elections::TypesFor,
		ethereum_elections::{
			BlockDataKeyManager, BlockDataScUtils, BlockDataStateChainGateway,
			BlockDataVaultDeposit, EthereumKeyManagerEvent, EthereumKeyManagerWitnessing,
			EthereumScUtilsWitnessing, EthereumStateChainGatewayWitnessing,
			EthereumVaultDepositWitnessing, EthereumVaultEvent, ScUtilsCall,
			StateChainGatewayEvent,
		},
	},
	EthereumBroadcaster, EthereumIngressEgress, Runtime,
};
use cf_chains::{instances::EthereumInstance, Chain, Ethereum};
use cf_traits::{FundAccount, FundingSource, Hook};
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::electoral_systems::block_witnesser::state_machine::{
	ExecuteHook, HookTypeFor, RulesHook,
};
use sp_std::{vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum EthEvent<T> {
	Witness(T),
}

type TypesVaultDepositWitnessing = TypesFor<EthereumVaultDepositWitnessing>;
type TypesStateChainGatewayWitnessing = TypesFor<EthereumStateChainGatewayWitnessing>;
type TypesKeyManagerWitnessing = TypesFor<EthereumKeyManagerWitnessing>;
type TypesScUtilsWitnessing = TypesFor<EthereumScUtilsWitnessing>;
type BlockNumber = <Ethereum as Chain>::ChainBlockNumber;

impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<EthereumVaultEvent>)>) {
		for (block, event) in events {
			match event {
				EthEvent::Witness(call) => match call {
					EthereumVaultEvent::SwapNativeFilter(vault_deposit_witness) |
					EthereumVaultEvent::SwapTokenFilter(vault_deposit_witness) |
					EthereumVaultEvent::XcallNativeFilter(vault_deposit_witness) |
					EthereumVaultEvent::XcallTokenFilter(vault_deposit_witness) => {
						EthereumIngressEgress::process_vault_swap_request_full_witness(
							block,
							vault_deposit_witness,
						);
					},
					EthereumVaultEvent::TransferNativeFailedFilter {
						asset,
						amount,
						destination_address,
					} |
					EthereumVaultEvent::TransferTokenFailedFilter {
						asset,
						amount,
						destination_address,
					} => {
						EthereumIngressEgress::vault_transfer_failed_inner(
							asset,
							amount,
							destination_address,
						);
					},
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesStateChainGatewayWitnessing, ExecuteHook>>
	for TypesStateChainGatewayWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<StateChainGatewayEvent>)>) {
		for (_, event) in events {
			match event {
				EthEvent::Witness(call) => {
					match call {
						StateChainGatewayEvent::Funded { account_id, amount, funder, tx_hash } =>
							pallet_cf_funding::Pallet::<Runtime>::fund_account(
								account_id,
								amount,
								FundingSource::EthTransaction { tx_hash, funder },
							),
						StateChainGatewayEvent::RedemptionExecuted {
							account_id,
							redeemed_amount,
						} => {
							if let Err(err) = pallet_cf_funding::Pallet::<Runtime>::redeemed(
								account_id.clone(),
								redeemed_amount,
							) {
								log::error!(
									"Failed to execute Ethereum redemption: AccountId: {:?}, Amount: {:?}, Error: {:?}",
									account_id,
									redeemed_amount,
									err
								);
							}
						},
						StateChainGatewayEvent::RedemptionExpired {
							account_id,
							block_number: _,
						} => {
							if let Err(err) =
								pallet_cf_funding::Pallet::<Runtime>::redemption_expired(
									account_id.clone(),
								) {
								log::error!(
									"Failed to execute Ethereum redemption expiry: AccountId: {:?}, Error: {:?}",
									account_id,
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

impl_rules_hook!(TypesVaultDepositWitnessing, BlockDataVaultDeposit, EthEvent<EthereumVaultEvent>);
impl_rules_hook!(
	TypesStateChainGatewayWitnessing,
	BlockDataStateChainGateway,
	EthEvent<StateChainGatewayEvent>
);
impl_rules_hook!(TypesKeyManagerWitnessing, BlockDataKeyManager, EthEvent<EthereumKeyManagerEvent>);
impl_rules_hook!(TypesScUtilsWitnessing, BlockDataScUtils, EthEvent<ScUtilsCall>);
