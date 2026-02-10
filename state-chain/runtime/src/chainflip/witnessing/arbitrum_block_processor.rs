use crate::{
	chainflip::witnessing::{
		arbitrum_elections::{
			ArbitrumChain, ArbitrumDepositChannelWitnessing, BlockDataDepositChannel,
		},
		elections::TypesFor,
	},
	impl_rules_hook, ArbitrumIngressEgress,
};
use cf_chains::Arbitrum;
use cf_traits::Hook;
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{pallet_prelude::TypeInfo, Deserialize, Serialize};
use pallet_cf_elections::electoral_systems::{
	block_height_witnesser::ChainTypes,
	block_witnesser::state_machine::{ExecuteHook, HookTypeFor, RulesHook},
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

impl_rules_hook!(
	TypesDepositChannelWitnessing,
	BlockDataDepositChannel,
	ArbEvent<DepositWitness<Arbitrum>>
);
