use crate::{
	chainflip::{
		elections::TypesFor,
		ethereum_elections::{
			BlockDataDepositChannel, BlockDataKeyManager, BlockDataScUtils,
			BlockDataStateChainGateway, BlockDataVaultDeposit, EthereumDepositChannelWitnessing,
			EthereumKeyManagerWitnessing, EthereumScUtilsWitnessing,
			EthereumStateChainGatewayWitnessing, EthereumVaultDepositWitnessing, KeyManagerEvent,
			ScUtilsCall, StateChainGatewayEvent, VaultEvents,
		},
	},
	EthereumBroadcaster, EthereumIngressEgress, Runtime,
};
use cf_chains::{instances::EthereumInstance, Chain, Ethereum};
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
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
type TypesScUtilsWitnessing = TypesFor<EthereumScUtilsWitnessing>;
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
				EthEvent::Witness(call) => match call {
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
							pallet_cf_funding::Pallet::<Runtime>::fund_account(
								account_id, funder, amount, tx_hash,
							),
						StateChainGatewayEvent::RedemptionExecuted {
							account_id,
							redeemed_amount,
						} => {
							let _ = pallet_cf_funding::Pallet::<Runtime>::redeemed(
								account_id,
								redeemed_amount,
							);
						},
						StateChainGatewayEvent::RedemptionExpired {
							account_id,
							block_number: _,
						} => {
							let _ = pallet_cf_funding::Pallet::<Runtime>::redemption_expired(
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
						} => {
							#[allow(clippy::unit_arg)]
							if let Err(err) = EthereumBroadcaster::egress_success(
								pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
								tx_out_id,
								signer_id,
								tx_fee,
								tx_metadata,
								transaction_ref,
							) {
								log::error!(
									"Failed to execute Ethereum egress success: TxOutId: {:?}, Error: {:?}",
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

impl Hook<HookTypeFor<TypesScUtilsWitnessing, ExecuteHook>> for TypesScUtilsWitnessing {
	fn run(&mut self, events: Vec<(BlockNumber, EthEvent<ScUtilsCall>)>) {
		for (_, event) in dedup_events(events) {
			match event {
				EthEvent::PreWitness(_) => {},
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
		// No safety margin for egress success
		if age.contains(&0u32) {
			results.extend(
				block_data
					.clone()
					.into_iter()
					.filter_map(|event| match event {
						KeyManagerEvent::AggKeySetByGovKey { .. } |
						KeyManagerEvent::GovernanceAction { .. } => None,
						KeyManagerEvent::SignatureAccepted { .. } => Some(EthEvent::Witness(event)),
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
						KeyManagerEvent::GovernanceAction { .. } => Some(EthEvent::Witness(event)),
						KeyManagerEvent::SignatureAccepted { .. } => None,
					})
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}

impl Hook<HookTypeFor<TypesScUtilsWitnessing, RulesHook>> for TypesScUtilsWitnessing {
	fn run(
		&mut self,
		(age, block_data, safety_margin): (Range<u32>, BlockDataScUtils, u32),
	) -> Vec<EthEvent<ScUtilsCall>> {
		let mut results: Vec<EthEvent<ScUtilsCall>> = vec![];
		if age.contains(&safety_margin) {
			results.extend(
				block_data
					.iter()
					.map(|call| EthEvent::Witness(call.clone()))
					.collect::<Vec<_>>(),
			)
		}
		results
	}
}
