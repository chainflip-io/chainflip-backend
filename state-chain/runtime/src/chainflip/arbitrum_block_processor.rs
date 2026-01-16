use crate::{
	chainflip::{
		arbitrum_elections::{
			ArbitrumChain, ArbitrumDepositChannelWitnessing, ArbitrumKeyManagerEvent,
			ArbitrumKeyManagerWitnessing, ArbitrumVaultDepositWitnessing, ArbitrumVaultEvent,
			BlockDataDepositChannel, BlockDataKeyManager, BlockDataVaultDeposit,
		},
		elections::TypesFor,
	},
	ArbitrumBroadcaster, ArbitrumIngressEgress, Runtime,
};
use cf_chains::{instances::ArbitrumInstance, Arbitrum};
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_elections::electoral_systems::{
	block_height_witnesser::ChainTypes,
	block_witnesser::state_machine::{ExecuteHook, HookTypeFor, RulesHook},
	state_machine::core::Hook,
};
use pallet_cf_ingress_egress::DepositWitness;
use sp_std::{vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
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
							#[allow(clippy::unit_arg)]
							if let Err(err) = ArbitrumBroadcaster::egress_success(
								pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
								tx_out_id,
								signer_id,
								tx_fee,
								tx_metadata,
								transaction_ref,
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

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, RulesHook>> for TypesDepositChannelWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataDepositChannel, u32),
	) -> Vec<ArbEvent<DepositWitness<Arbitrum>>> {
		let mut results: Vec<ArbEvent<DepositWitness<Arbitrum>>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|deposit_witness| ArbEvent::Witness(deposit_witness.clone()))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

impl Hook<HookTypeFor<TypesVaultDepositWitnessing, RulesHook>> for TypesVaultDepositWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataVaultDeposit, u32),
	) -> Vec<ArbEvent<ArbitrumVaultEvent>> {
		let mut results: Vec<ArbEvent<ArbitrumVaultEvent>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| ArbEvent::Witness(vault_deposit.clone()))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

impl Hook<HookTypeFor<TypesKeyManagerWitnessing, RulesHook>> for TypesKeyManagerWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataKeyManager, u32),
	) -> Vec<ArbEvent<ArbitrumKeyManagerEvent>> {
		let mut results: Vec<ArbEvent<ArbitrumKeyManagerEvent>> = vec![];
		// No safety margin for egress success
		if age.contains(&0u32) {
			results.extend(
				block_data
					.clone()
					.into_iter()
					.filter_map(|event| match event {
						ArbitrumKeyManagerEvent::AggKeySetByGovKey { .. } |
						ArbitrumKeyManagerEvent::GovernanceAction { .. } => None,
						ArbitrumKeyManagerEvent::SignatureAccepted { .. } =>
							Some(ArbEvent::Witness(event)),
					})
					.collect::<Vec<_>>(),
			)
		}
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.into_iter()
					.filter_map(|event| match event {
						ArbitrumKeyManagerEvent::AggKeySetByGovKey { .. } |
						ArbitrumKeyManagerEvent::GovernanceAction { .. } => Some(ArbEvent::Witness(event)),
						ArbitrumKeyManagerEvent::SignatureAccepted { .. } => None,
					})
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}
