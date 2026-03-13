// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use core::ops::RangeInclusive;

use crate::{
	chainflip::{
		witnessing::pallet_hooks::{self, EvmKeyManagerEvent, EvmVaultContractEvent},
		ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	Runtime, TronChainTracking, TronIngressEgress,
};
use cf_chains::{
	evm, instances::TronInstance, witness_period::SaturatingStep, Chain, DepositChannel, Tron,
};
use cf_traits::{impl_pallet_safe_mode, Chainflip, Hook};
use cf_utilities::{define_empty_struct, impls};
use codec::DecodeWithMemTracking;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::{
		block_height_witnesser::{
			consensus::BlockHeightWitnesserConsensus, primitives::NonemptyContinuousHeaders,
			state_machine::BlockHeightWitnesser, BHWTypes, BlockHeightChangeHook,
			BlockHeightWitnesserSettings, ChainBlockNumberOf, ChainProgress, ChainTypes, ReorgHook,
		},
		block_witnesser::{
			instance::{BlockWitnesserInstance, GenericBlockWitnesser, JustWitnessAtSafetyMargin},
			state_machine::{BWElectionType, BlockWitnesserSettings, HookTypeFor},
		},
		composite::{
			tuple_5_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::state_machine_es::{
			StatemachineElectoralSystem, StatemachineElectoralSystemTypes,
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, ElectoralSystemTypes, InitialState,
	InitialStateOf, RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

pub type TronElectoralSystemRunner = CompositeRunner<
	(
		TronBlockHeightWitnesserES,
		TronDepositChannelWitnessingES,
		TronVaultDepositWitnessingES,
		TronKeyManagerWitnessingES,
		TronLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, TronInstance>,
	TronElectionHooks,
>;

define_empty_struct! { pub struct TronChain; }

impl ChainTypes for TronChain {
	type ChainBlockNumber = <Tron as Chain>::ChainBlockNumber;
	type ChainBlockHash = evm::H256;
	const NAME: &'static str = "Tron";
}

pub const TRON_MAINNET_SAFETY_BUFFER: u32 = 1;

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo)]
pub enum TronElectoralEvents {
	ReorgDetected { reorged_blocks: RangeInclusive<<Tron as Chain>::ChainBlockNumber> },
}

// ------------------------ block height tracking ---------------------------

define_empty_struct! { pub struct TronBlockHeightWitnesser; }

impls! {
	for TronBlockHeightWitnesser:

	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = TronChain;
		type ReorgHook = Self;
	}

	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<TronChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<TronChain>>;

		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: <Tron as Chain>::ChainBlockNumber) {
			if let Err(err) = TronChainTracking::inner_update_chain_height(block_height) {
				log::error!("Failed to update TRON chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<<Tron as Chain>::ChainBlockNumber>) {
			pallet_cf_elections::Pallet::<Runtime, TronInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(TronElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

pub type TronBlockHeightWitnesserES = StatemachineElectoralSystem<TronBlockHeightWitnesser>;

// ------------------------ deposit channel witnessing ---------------------------

define_empty_struct! { pub struct TronDepositChannelWitnessing; }

impl BlockWitnesserInstance for TronDepositChannelWitnessing {
	const BWNAME: &'static str = "DepositChannel";
	type Runtime = Runtime;
	type Chain = TronChain;
	type BlockEntry = DepositWitness<Tron>;
	type ElectionProperties = Vec<DepositChannel<Tron>>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, TronInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<TronInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<TronInstance>,
		>>::get()
		.deposit_channel_witnessing_enabled
	}

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties {
		TronIngressEgress::active_deposit_channels_at(
			height.saturating_forward(TRON_MAINNET_SAFETY_BUFFER as usize),
			height,
		)
		.into_iter()
		.map(|deposit_channel_details| deposit_channel_details.deposit_channel)
		.collect()
	}

	fn processed_up_to(up_to: ChainBlockNumberOf<Self::Chain>) {
		ProcessedUpTo::<Runtime, TronInstance>::set(
			up_to.saturating_backward(TRON_MAINNET_SAFETY_BUFFER as usize),
		);
	}
}

pub type TronDepositChannelWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TronDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------

define_empty_struct! { pub struct TronVaultDepositWitnessing; }

impl BlockWitnesserInstance for TronVaultDepositWitnessing {
	const BWNAME: &'static str = "VaultDeposit";
	type Runtime = Runtime;
	type Chain = TronChain;
	type BlockEntry = EvmVaultContractEvent<Runtime, TronInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, TronInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<TronInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<TronInstance>,
		>>::get()
		.vault_deposit_witnessing_enabled
	}

	fn election_properties(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// Vault address doesn't change, it is read by the engine on startup
	}

	fn processed_up_to(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

pub type TronVaultDepositWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TronVaultDepositWitnessing>>;

// ------------------------ Key Manager witnessing ---------------------------

define_empty_struct! { pub struct TronKeyManagerWitnessing; }

impl BlockWitnesserInstance for TronKeyManagerWitnessing {
	const BWNAME: &'static str = "KeyManager";
	type Runtime = Runtime;
	type Chain = TronChain;
	type BlockEntry = EvmKeyManagerEvent<Runtime, TronInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, TronInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_elections::Config<TronInstance>>::SafeMode as Get<
			TronElectionsSafeMode,
		>>::get()
		.key_manager_witnessing
	}

	fn election_properties(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// KeyManager address doesn't change, it is read by the engine on startup
	}

	fn processed_up_to(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP
	}
}

pub type TronKeyManagerWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TronKeyManagerWitnessing>>;

// ------------------------ liveness ---------------------------

pub type TronLiveness = Liveness<
	<Tron as Chain>::ChainBlockNumber,
	evm::H256,
	ReportFailedLivenessCheck<Tron>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ election hooks ---------------------------

pub struct TronElectionHooks;

impl
	Hooks<
		TronBlockHeightWitnesserES,
		TronDepositChannelWitnessingES,
		TronVaultDepositWitnessingES,
		TronKeyManagerWitnessingES,
		TronLiveness,
	> for TronElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, key_manager_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<TronBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<TronDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<TronVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<TronKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<TronLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let current_sc_block_number = crate::System::block_number();

		let chain_progress = TronBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				TronBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, TronInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

		TronDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				TronDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, TronInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress.clone())?;

		TronVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				TronVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, TronInstance>,
			>,
		>(vault_deposits_identifiers, &chain_progress.clone())?;

		TronKeyManagerWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				TronKeyManagerWitnessingES,
				RunnerStorageAccess<Runtime, TronInstance>,
			>,
		>(key_manager_identifiers, &chain_progress.clone())?;

		TronLiveness::on_finalize::<
			DerivedElectoralAccess<_, TronLiveness, RunnerStorageAccess<Runtime, TronInstance>>,
		>(
			liveness_identifiers,
			&(
				current_sc_block_number,
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, TronInstance>::get()
					.unwrap()
					.block_height
					.saturating_sub(TRON_MAINNET_SAFETY_BUFFER.into()),
				crate::Validator::current_epoch(),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, TronInstance> {
	InitialState {
		unsynchronised_state: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		),
		unsynchronised_settings: (
			BlockHeightWitnesserSettings { safety_buffer: TRON_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
			},
			(),
		),
		settings: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			LIVENESS_CHECK_DURATION,
		),
		shared_data_reference_lifetime: 8,
	}
}

impl_pallet_safe_mode! {
	TronElectionsSafeMode;

	key_manager_witnessing,
}

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(<TronDepositChannelWitnessing as BlockWitnesserInstance>::ElectionProperties),
	Vaults(()),
	KeyManager(()),
}

pub struct ElectoralSystemConfiguration;
impl pallet_cf_elections::ElectoralSystemConfiguration for ElectoralSystemConfiguration {
	type SafeMode = TronElectionsSafeMode;

	type ElectoralEvents = TronElectoralEvents;

	type Properties = (<TronChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, TronInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _)| {
							state
								.1
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(channels));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create governance election with properties: {properties:?}");
				}
			},
			ElectionTypes::Vaults(_) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, TronInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _)| {
							state
								.2
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create vault witnessing governance election with properties for block {block_height:?}");
				},
			ElectionTypes::KeyManager(_) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, TronInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _)| {
							state
								.3
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create key manager witnessing governance election with properties for block {block_height:?}");
				},
		}
	}
}
