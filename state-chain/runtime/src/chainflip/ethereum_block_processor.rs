use core::ops::Range;

use crate::{
	chainflip::{
		elections::TypesFor,
		ethereum_elections::{
			BlockDataDepositChannel, BlockDataKeyManager, BlockDataStateChainGateway,
			BlockDataVaultDeposit, EgressBlockData, EthereumDepositChannelWitnessing,
			EthereumEgressWitnessing, EthereumKeyManagerWitnessing,
			EthereumStateChainGatewayWitnessing, EthereumVaultDepositWitnessing, KeyManagerEvent,
			StateChainGatewayEvent, VaultEvents,
		},
	},
	EthereumBroadcaster, EthereumIngressEgress, Runtime,
};
use cf_chains::{instances::EthereumInstance, Chain, Ethereum};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use frame_system::pallet_prelude::OriginFor;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::state_machine::{ExecuteHook, HookTypeFor, RulesHook},
	state_machine::core::Hook,
};
use pallet_cf_ingress_egress::DepositWitness;
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum EthEvent<T> {
	PreWitness(T),
	Witness(T),
}
impl<T> EthEvent<T> {
	fn inner_witness(&self) -> &T {
		match self {
			EthEvent::PreWitness(w) | EthEvent::Witness(w) => w,
		}
	}
}

type TypesDepositChannelWitnessing = TypesFor<EthereumDepositChannelWitnessing>;
type TypesVaultDepositWitnessing = TypesFor<EthereumVaultDepositWitnessing>;
type TypesStateChainGatewayWitnessing = TypesFor<EthereumStateChainGatewayWitnessing>;
type TypesKeyManagerWitnessing = TypesFor<EthereumKeyManagerWitnessing>;
type TypesEgressWitnessing = TypesFor<EthereumEgressWitnessing>;
type BlockNumber = <Ethereum as Chain>::ChainBlockNumber;

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - keep only the `Witness` variant,
fn dedup_events<T: Ord + Clone>(
	events: Vec<(BlockNumber, EthEvent<T>)>,
) -> Vec<(BlockNumber, EthEvent<T>)> {
	let mut chosen: BTreeMap<T, (BlockNumber, EthEvent<T>)> = BTreeMap::new();

	for (block, event) in events {
		let witness = event.inner_witness().clone();

		// Only insert if no event exists yet, or if we're upgrading from PreWitness to Witness
		if !chosen.contains_key(&witness) ||
			(matches!(chosen.get(&witness), Some((_, EthEvent::PreWitness(_)))) &&
				matches!(event, EthEvent::Witness(_)))
		{
			chosen.insert(witness, (block, event));
		}
	}

	chosen.into_values().collect()
}

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, ExecuteHook>>
	for TypesDepositChannelWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<DepositWitness<Ethereum>>)>) {
		let deduped_events = dedup_events(events);
		for (block, event) in &deduped_events {
			match event {
				EthEvent::PreWitness(_) => {},
				EthEvent::Witness(deposit) => {
					EthereumIngressEgress::process_channel_deposit_full_witness(
						deposit.clone(),
						*block,
					);
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesVaultDepositWitnessing, ExecuteHook>> for TypesVaultDepositWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<VaultEvents>)>) {
		for (block, event) in &dedup_events(events) {
			match event {
				EthEvent::PreWitness(_) => {},
				EthEvent::Witness(call) => {
					match call {
						VaultEvents::SwapNativeFilter(vault_deposit_witness) |
						VaultEvents::SwapTokenFilter(vault_deposit_witness) |
						VaultEvents::XcallNativeFilter(vault_deposit_witness) |
						VaultEvents::XcallTokenFilter(vault_deposit_witness) => {
							EthereumIngressEgress::process_vault_swap_request_full_witness(
								*block,
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
							EthereumIngressEgress::vault_transfer_failed_inner(
								*asset,
								*amount,
								*destination_address,
							);
						},
					}
				},
			}
		}
	}
}
impl Hook<HookTypeFor<TypesStateChainGatewayWitnessing, ExecuteHook>>
	for TypesStateChainGatewayWitnessing
{
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<StateChainGatewayEvent>)>) {
		for (_, event) in dedup_events(events) {
			match event {
				EthEvent::PreWitness(_) => {},
				EthEvent::Witness(call) => {
					match call {
						StateChainGatewayEvent::Funded { account_id, amount, funder, tx_hash } =>
							pallet_cf_funding::Pallet::<Runtime>::inner_funded(
								account_id, amount, funder, tx_hash,
							),
						StateChainGatewayEvent::RedemptionExecuted {
							account_id,
							redeemed_amount,
							tx_hash: _,
						} => {
							let _ = pallet_cf_funding::Pallet::<Runtime>::inner_redeemed(
								account_id,
								redeemed_amount,
							);
						},
						StateChainGatewayEvent::RedemptionExpired {
							account_id,
							block_number: _,
						} => {
							let _ = pallet_cf_funding::Pallet::<Runtime>::inner_redemption_expired(
								account_id,
							);
						},
					};
				},
			};
		}
	}
}
impl Hook<HookTypeFor<TypesKeyManagerWitnessing, ExecuteHook>> for TypesKeyManagerWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<KeyManagerEvent>)>) {
		for (_, event) in dedup_events(events) {
			match event {
				EthEvent::PreWitness(_) => {},
				EthEvent::Witness(call) => {
					match call {
						KeyManagerEvent::AggKeySetByGovKey {
							new_public_key,
							block_number,
							tx_id: _,
						} => {
							pallet_cf_vaults::Pallet::<Runtime, EthereumInstance>::inner_vault_key_rotated_externally(new_public_key, block_number);
						},
						KeyManagerEvent::SignatureAccepted {
							tx_out_id,
							signer_id,
							tx_fee,
							tx_metadata,
							transaction_ref,
							// TODO: Check that the origin used works
							// If not we can use root origin? =>
							// frame_system::RawOrigin::Root.into()
						} => {
							let _ = pallet_cf_broadcast::Pallet::<Runtime, EthereumInstance>::egress_success(OriginFor::<Runtime>::root(), tx_out_id, signer_id, tx_fee, tx_metadata, transaction_ref);
						},
						KeyManagerEvent::GovernanceAction {
							call_hash,
							// TODO: Same as above, check that origin works and if not create inner
							// function without origin
						} => {
							let _ =
								pallet_cf_governance::Pallet::<Runtime>::set_whitelisted_call_hash(
									OriginFor::<Runtime>::root(),
									call_hash,
								);
						},
					};
				},
			};
		}
	}
}
impl Hook<HookTypeFor<TypesEgressWitnessing, ExecuteHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		events: Vec<(BlockNumber, EthEvent<TransactionConfirmation<Runtime, EthereumInstance>>)>,
	) {
		let deduped_events = dedup_events(events);
		for (_, event) in &deduped_events {
			match event {
				EthEvent::PreWitness(_) => { /* We don't care about pre-witnessing an egress*/ },
				EthEvent::Witness(egress) => {
					EthereumBroadcaster::broadcast_success(egress.clone());
				},
			}
		}
	}
}

impl Hook<HookTypeFor<TypesDepositChannelWitnessing, RulesHook>> for TypesDepositChannelWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataDepositChannel, u32),
	) -> Vec<EthEvent<DepositWitness<Ethereum>>> {
		let mut results: Vec<EthEvent<DepositWitness<Ethereum>>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|deposit_witness| EthEvent::Witness(deposit_witness.clone()))
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
	) -> Vec<EthEvent<VaultEvents>> {
		let mut results: Vec<EthEvent<VaultEvents>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|vault_deposit| EthEvent::Witness(vault_deposit.clone()))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}
impl Hook<HookTypeFor<TypesStateChainGatewayWitnessing, RulesHook>>
	for TypesStateChainGatewayWitnessing
{
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataStateChainGateway, u32),
	) -> Vec<EthEvent<StateChainGatewayEvent>> {
		let mut results: Vec<EthEvent<StateChainGatewayEvent>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(block_data.into_iter().map(EthEvent::Witness).collect::<Vec<_>>())
		}
		results
	}
}

impl Hook<HookTypeFor<TypesKeyManagerWitnessing, RulesHook>> for TypesKeyManagerWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataKeyManager, u32),
	) -> Vec<EthEvent<KeyManagerEvent>> {
		let mut results: Vec<EthEvent<KeyManagerEvent>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(block_data.into_iter().map(EthEvent::Witness).collect::<Vec<_>>())
		}
		results
	}
}

impl Hook<HookTypeFor<TypesEgressWitnessing, RulesHook>> for TypesEgressWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, EgressBlockData, u32),
	) -> Vec<EthEvent<TransactionConfirmation<Runtime, EthereumInstance>>> {
		if age.contains(&safety_margin) {
			return block_data
				.iter()
				.map(|egress_witness| EthEvent::Witness(egress_witness.clone()))
				.collect::<Vec<_>>();
		}
		vec![]
	}
}
