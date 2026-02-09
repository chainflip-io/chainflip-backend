use crate::{
	chainflip::witnessing::{
		bsc_elections::{
			BlockDataDepositChannel, BlockDataKeyManager, BlockDataVaultDeposit, BscChain,
			BscDepositChannelWitnessing, BscKeyManagerEvent, BscKeyManagerWitnessing,
			BscVaultDepositWitnessing, BscVaultEvent,
		},
		elections::TypesFor,
	},
	impl_rules_hook, BscBroadcaster, BscIngressEgress, Runtime,
};
use cf_chains::{instances::BscInstance, Bsc};
use cf_traits::Hook;
use codec::{Decode, Encode};
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
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum BscEvent<T> {
	Witness(T),
}

type TypesDepositChannelWitnessing = TypesFor<BscDepositChannelWitnessing>;
type TypesVaultDepositWitnessing = TypesFor<BscVaultDepositWitnessing>;
type TypesKeyManagerWitnessing = TypesFor<BscKeyManagerWitnessing>;
type BlockNumber = <BscChain as ChainTypes>::ChainBlockNumber;

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, ExecuteHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, BscEvent<DepositWitness<Bsc>>)>) {
		for (block, event) in events {
			match event {
				BscEvent::Witness(deposit) => {
					BscIngressEgress::process_channel_deposit_full_witness(deposit, *block.root());
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, BscEvent<BscVaultEvent>)>) {
		for (block, event) in events {
			match event {
				BscEvent::Witness(call) => match call {
					BscVaultEvent::SwapNativeFilter(vault_deposit_witness) |
					BscVaultEvent::SwapTokenFilter(vault_deposit_witness) |
					BscVaultEvent::XcallNativeFilter(vault_deposit_witness) |
					BscVaultEvent::XcallTokenFilter(vault_deposit_witness) => {
						BscIngressEgress::process_vault_swap_request_full_witness(
							*block.root(),
							vault_deposit_witness,
						);
					},
					BscVaultEvent::TransferNativeFailedFilter {
						asset,
						amount,
						destination_address,
					} |
					BscVaultEvent::TransferTokenFailedFilter {
						asset,
						amount,
						destination_address,
					} => {
						BscIngressEgress::vault_transfer_failed_inner(
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
	fn run(&mut self, events: Vec<(BlockNumber, BscEvent<BscKeyManagerEvent>)>) {
		for (block_number, event) in events {
			match event {
				BscEvent::Witness(call) => {
					match call {
						BscKeyManagerEvent::AggKeySetByGovKey {
							new_public_key,
							block_number,
							tx_id: _,
						} => {
							pallet_cf_vaults::Pallet::<Runtime, BscInstance>::inner_vault_key_rotated_externally(new_public_key, block_number);
						},
						BscKeyManagerEvent::SignatureAccepted {
							tx_out_id,
							signer_id,
							tx_fee,
							tx_metadata,
							transaction_ref,
						} => {
							if let Err(err) = BscBroadcaster::egress_success(
								pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
								TransactionConfirmation {
									tx_out_id,
									signer_id,
									tx_fee,
									tx_metadata,
									transaction_ref,
								},
							*block_number.root(),
							) {
								log::error!(
									"Failed to execute BSC egress success: TxOutId: {:?}, Error: {:?}",
									tx_out_id,
									err
								)
							}
						},
						BscKeyManagerEvent::GovernanceAction {
							// TODO: Same as above, check that origin works and if not create inner
							// function without origin
							call_hash,
						} => {
							if let Err(err) =
								pallet_cf_governance::Pallet::<Runtime>::set_whitelisted_call_hash(
									pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold
										.into(),
									call_hash,
								) {
								log::error!(
									"Failed to whitelist BSC governance call hash: {:?}, Error: {:?}",
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
	BscEvent<DepositWitness<Bsc>>
);
impl_rules_hook!(TypesVaultDepositWitnessing, BlockDataVaultDeposit, BscEvent<BscVaultEvent>);
impl_rules_hook!(TypesKeyManagerWitnessing, BlockDataKeyManager, BscEvent<BscKeyManagerEvent>);
