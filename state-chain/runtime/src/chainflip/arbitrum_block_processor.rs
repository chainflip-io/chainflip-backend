use crate::{
	chainflip::{
		arbitrum_elections::{
			ArbitrumChain, ArbitrumDepositChannelWitnessing, ArbitrumKeyManagerWitnessing,
			ArbitrumVaultDepositWitnessing, BlockDataDepositChannel, BlockDataKeyManager,
			BlockDataVaultDeposit, KeyManagerEvent, VaultEvents,
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
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum ArbEvent<T> {
	PreWitness(T),
	Witness(T),
}
impl<T> ArbEvent<T> {
	fn inner_witness(&self) -> &T {
		match self {
			ArbEvent::PreWitness(w) | ArbEvent::Witness(w) => w,
		}
	}
}

type TypesDepositChannelWitnessing = TypesFor<ArbitrumDepositChannelWitnessing>;
type TypesVaultDepositWitnessing = TypesFor<ArbitrumVaultDepositWitnessing>;
type TypesKeyManagerWitnessing = TypesFor<ArbitrumKeyManagerWitnessing>;
type BlockNumber = <ArbitrumChain as ChainTypes>::ChainBlockNumber;

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep only the `Witness` variant,
fn dedup_events<T: Ord + Clone>(
	events: Vec<(BlockNumber, ArbEvent<T>)>,
) -> Vec<(BlockNumber, ArbEvent<T>)> {
	let mut chosen: BTreeMap<T, (BlockNumber, ArbEvent<T>)> = BTreeMap::new();

	for (block, event) in events {
		let witness = event.inner_witness().clone();

		// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
		if !chosen.contains_key(&witness) ||
			(matches!(chosen.get(&witness), Some((_, ArbEvent::PreWitness(_)))) &&
				matches!(event, ArbEvent::Witness(_)))
		{
			chosen.insert(witness, (block, event));
		}
	}

	chosen.into_values().collect()
}

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, ExecuteHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<DepositWitness<Arbitrum>>)>) {
		let deduped_events = dedup_events(events);
		for (block, event) in &deduped_events {
			match event {
				ArbEvent::PreWitness(_) => {},
				ArbEvent::Witness(deposit) => {
					ArbitrumIngressEgress::process_channel_deposit_full_witness(
						deposit.clone(),
						*block.root(),
					);
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<VaultEvents>)>) {
		for (block, event) in &dedup_events(events) {
			match event {
				ArbEvent::PreWitness(_) => {},
				ArbEvent::Witness(call) => match call {
					VaultEvents::SwapNativeFilter(vault_deposit_witness) |
					VaultEvents::SwapTokenFilter(vault_deposit_witness) |
					VaultEvents::XcallNativeFilter(vault_deposit_witness) |
					VaultEvents::XcallTokenFilter(vault_deposit_witness) => {
						ArbitrumIngressEgress::process_vault_swap_request_full_witness(
							*block.root(),
							vault_deposit_witness.clone(),
						);
					},
					VaultEvents::TransferNativeFailedFilter {
						asset,
						amount,
						destination_address,
					} |
					VaultEvents::TransferTokenFailedFilter {
						asset,
						amount,
						destination_address,
					} => {
						ArbitrumIngressEgress::vault_transfer_failed_inner(
							*asset,
							*amount,
							*destination_address,
						);
					},
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesKeyManagerWitnessing, ExecuteHook>> for TypesKeyManagerWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, ArbEvent<KeyManagerEvent>)>) {
		for (_, event) in dedup_events(events) {
			match event {
				ArbEvent::PreWitness(_) => {},
				ArbEvent::Witness(call) => {
					match call {
						KeyManagerEvent::AggKeySetByGovKey {
							new_public_key,
							block_number,
							tx_id: _,
						} => {
							pallet_cf_vaults::Pallet::<Runtime, ArbitrumInstance>::inner_vault_key_rotated_externally(new_public_key, block_number);
						},
						KeyManagerEvent::SignatureAccepted {
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
						KeyManagerEvent::GovernanceAction {
							call_hash,
							// TODO: Same as above, check that origin works and if not create inner
							// function without origin
						} => {
							let _ =
								pallet_cf_governance::Pallet::<Runtime>::set_whitelisted_call_hash(
									pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold
										.into(),
									call_hash,
								);
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
	) -> Vec<ArbEvent<VaultEvents>> {
		let mut results: Vec<ArbEvent<VaultEvents>> = vec![];
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
	) -> Vec<ArbEvent<KeyManagerEvent>> {
		let mut results: Vec<ArbEvent<KeyManagerEvent>> = vec![];
		// No safety margin for egress success
		if age.contains(&0u32) {
			results.extend(
				block_data
					.clone()
					.into_iter()
					.filter_map(|event| match event {
						KeyManagerEvent::AggKeySetByGovKey { .. } |
						KeyManagerEvent::GovernanceAction { .. } => None,
						KeyManagerEvent::SignatureAccepted { .. } => Some(ArbEvent::Witness(event)),
					})
					.collect::<Vec<_>>(),
			)
		}
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.into_iter()
					.filter_map(|event| match event {
						KeyManagerEvent::AggKeySetByGovKey { .. } |
						KeyManagerEvent::GovernanceAction { .. } => Some(ArbEvent::Witness(event)),
						KeyManagerEvent::SignatureAccepted { .. } => None,
					})
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}
