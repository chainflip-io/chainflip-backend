use crate::{
	chainflip::witnessing::{
		arbitrum_elections::{
			ArbitrumChain, ArbitrumDepositChannelWitnessing, ArbitrumKeyManagerEvent,
			ArbitrumKeyManagerWitnessing, ArbitrumVaultDepositWitnessing, ArbitrumVaultEvent,
			BlockDataDepositChannel, BlockDataKeyManager, BlockDataVaultDeposit,
		},
		elections::TypesFor,
	},
	impl_rules_hook, ArbitrumBroadcaster, ArbitrumIngressEgress, Runtime,
};
use cf_chains::{instances::ArbitrumInstance, Arbitrum};
use cf_traits::Hook;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::electoral_systems::{
	block_height_witnesser::ChainTypes,
	block_witnesser::state_machine::{ExecuteHook, HookTypeFor, RulesHook},
};
use pallet_cf_ingress_egress::DepositWitness;
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
pub enum ArbEvent<T> {
	Witness(T),
}

type TypesDepositChannelWitnessing = TypesFor<ArbitrumDepositChannelWitnessing>;
type TypesVaultDepositWitnessing = TypesFor<ArbitrumVaultDepositWitnessing>;
type TypesKeyManagerWitnessing = TypesFor<ArbitrumKeyManagerWitnessing>;
type BlockNumber = <ArbitrumChain as ChainTypes>::ChainBlockNumber;

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, ExecuteHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<DepositWitness<Arbitrum>>)>) {
		for (block, event) in events {
			match event {
				ArbEvent::Witness(deposit) => {
					ArbitrumIngressEgress::process_channel_deposit_full_witness(
						deposit,
						*block.root(),
					);
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<ArbitrumVaultEvent>)>) {
		for (block, event) in events {
			match event {
				ArbEvent::Witness(call) => match call {
					ArbitrumVaultEvent::SwapNativeFilter(vault_deposit_witness) |
					ArbitrumVaultEvent::SwapTokenFilter(vault_deposit_witness) |
					ArbitrumVaultEvent::XcallNativeFilter(vault_deposit_witness) |
					ArbitrumVaultEvent::XcallTokenFilter(vault_deposit_witness) => {
						ArbitrumIngressEgress::process_vault_swap_request_full_witness(
							*block.root(),
							vault_deposit_witness,
						);
					},
					ArbitrumVaultEvent::TransferNativeFailedFilter {
						asset,
						amount,
						destination_address,
					} |
					ArbitrumVaultEvent::TransferTokenFailedFilter {
						asset,
						amount,
						destination_address,
					} => {
						ArbitrumIngressEgress::vault_transfer_failed_inner(
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
impl Hook<HookTypeFor<TypesKeyManagerWitnessing, ExecuteHook>> for TypesKeyManagerWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<ArbitrumKeyManagerEvent>)>) {
		for (_, event) in events {
			match event {
				ArbEvent::Witness(call) => {
					match call {
						ArbitrumKeyManagerEvent::AggKeySetByGovKey {
							new_public_key,
							block_number,
							tx_id: _,
						} => {
							pallet_cf_vaults::Pallet::<Runtime, ArbitrumInstance>::inner_vault_key_rotated_externally(new_public_key, block_number);
						},
						ArbitrumKeyManagerEvent::SignatureAccepted {
							tx_out_id,
							signer_id,
							tx_fee,
							tx_metadata,
							transaction_ref,
						} => {
							if let Err(err) = ArbitrumBroadcaster::egress_success(
								pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
								TransactionConfirmation {
									tx_out_id,
									signer_id,
									tx_fee,
									tx_metadata,
									transaction_ref,
								},
							) {
								log::error!(
									"Failed to execute Arbitrum egress success: TxOutId: {:?}, Error: {:?}",
									tx_out_id,
									err
								)
							}
						},
						ArbitrumKeyManagerEvent::GovernanceAction {
							call_hash,
							// TODO: Same as above, check that origin works and if not create inner
							// function without origin
						} => {
							if let Err(err) =
								pallet_cf_governance::Pallet::<Runtime>::set_whitelisted_call_hash(
									pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold
										.into(),
									call_hash,
								) {
								log::error!(
									"Failed to whitelist Arbitrum governance call hash: {:?}, Error: {:?}",
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

impl_rules_hook!(
	TypesDepositChannelWitnessing,
	BlockDataDepositChannel,
	ArbEvent<DepositWitness<Arbitrum>>
);
impl_rules_hook!(TypesVaultDepositWitnessing, BlockDataVaultDeposit, ArbEvent<ArbitrumVaultEvent>);
impl_rules_hook!(TypesKeyManagerWitnessing, BlockDataKeyManager, ArbEvent<ArbitrumKeyManagerEvent>);
