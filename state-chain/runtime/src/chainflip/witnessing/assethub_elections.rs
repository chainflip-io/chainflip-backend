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
use crate::{
	chainflip::{witnessing::pallet_hooks, ReportFailedLivenessCheck},
	constants::common::LIVENESS_CHECK_DURATION,
	AssethubChainTracking, AssethubIngressEgress, Runtime,
};
use cf_chains::{
	dot::PolkadotSignature,
	hub::AssethubTrackedData,
	instances::AssethubInstance,
	witness_period::{BlockWitnessRange, SaturatingStep},
	Assethub, DepositChannel,
};
use cf_traits::{Chainflip, Hook};
use cf_utilities::{define_empty_struct, impls};
use codec::DecodeWithMemTracking;
use core::ops::RangeInclusive;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{TransactionConfirmation, TransactionOutIdToBroadcastId};
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::{
		block_height_witnesser::{
			consensus::BlockHeightWitnesserConsensus, primitives::NonemptyContinuousHeaders,
			state_machine::BlockHeightWitnesser, BHWTypes, BlockHeightChangeHook,
			BlockHeightWitnesserSettings, ChainBlockNumberOf, ChainProgress, ChainTypes, ReorgHook,
		},
		block_witnesser::{
			instance::{BlockWitnesserInstance, GenericBlockWitnesser, JustWitnessAtSafetyMargin},
			state_machine::{BWElectionType, BWTypes, BlockWitnesserSettings, HookTypeFor},
		},
		composite::{
			tuple_5_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::state_machine_es::{
			StatemachineElectoralSystem, StatemachineElectoralSystemTypes,
		},
		unsafe_median::{UnsafeMedian, UpdateFeeHook},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

pub type AssethubElectoralSystemRunner = CompositeRunner<
	(
		AssethubBlockHeightWitnesserES,
		AssethubDepositChannelWitnessingES,
		AssethubEgressWitnessingES,
		AssethubFeeTracking,
		AssethubLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, AssethubInstance>,
	AssethubElectionHooks,
>;

pub type AssethubWitnessBatchNumber = BlockWitnessRange<Assethub>;

define_empty_struct! { pub struct AssethubChain; }
impl ChainTypes for AssethubChain {
	type ChainBlockNumber = AssethubWitnessBatchNumber;
	// block numbers are unique identifiers because we only use finalized blocks,
	// so we can use them for the purpose of "hashes"
	type ChainBlockHash = AssethubWitnessBatchNumber;
	const NAME: &'static str = "Assethub";
}

pub const ASSETHUB_MAINNET_SAFETY_BUFFER: u32 = 1; // we witness finalized blocks so we don't need a safety buffer
pub const ASSETHUB_MAX_SUBMIT_HEADERS_IN_BHW_VOTER: u32 = 8;

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo)]
pub enum AssethubElectoralEvents {
	ReorgDetected { reorged_blocks: RangeInclusive<AssethubWitnessBatchNumber> },
}
// ------------------------ block height tracking ---------------------------
// The electoral system for block height tracking
define_empty_struct! { pub struct AssethubBlockHeightWitnesser; }

impls! {
	for AssethubBlockHeightWitnesser:

	/// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = AssethubChain;
		type ReorgHook = Self;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<AssethubChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<AssethubChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: AssethubWitnessBatchNumber) {
			if let Err(err) = AssethubChainTracking::inner_update_chain_height(*block_height.root()) {
				log::error!("Failed to update Assethub chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<AssethubWitnessBatchNumber>) {
			pallet_cf_elections::Pallet::<Runtime, AssethubInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(AssethubElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

/// Generating the state machine-based electoral system
pub type AssethubBlockHeightWitnesserES = StatemachineElectoralSystem<AssethubBlockHeightWitnesser>;

// ------------------------ deposit channel witnessing ---------------------------
// The electoral system for deposit channel witnessing
define_empty_struct! { pub struct AssethubDepositChannelWitnessing; }

impl BlockWitnesserInstance for AssethubDepositChannelWitnessing {
	const BWNAME: &'static str = "DepositChannel";
	type Runtime = Runtime;
	type Chain = AssethubChain;
	type BlockEntry = DepositWitness<Assethub>;
	type ElectionProperties = Vec<DepositChannel<Assethub>>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, AssethubInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<AssethubInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
		>>::get()
		.deposit_channel_witnessing_enabled
	}

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties {
		let height = height.root();
		AssethubIngressEgress::active_deposit_channels_at(
			// we advance by SAFETY_BUFFER before checking opened_at
			height.saturating_forward(ASSETHUB_MAINNET_SAFETY_BUFFER as usize),
			// we don't advance for expiry
			*height,
		)
		.into_iter()
		.map(|deposit_channel_details| deposit_channel_details.deposit_channel)
		.collect()
	}

	fn processed_up_to(up_to: ChainBlockNumberOf<Self::Chain>) {
		// we go back SAFETY_BUFFER, such that we only actually expire once this amount of blocks
		// have been additionally processed.
		ProcessedUpTo::<Runtime, AssethubInstance>::set(
			up_to.root().saturating_backward(ASSETHUB_MAINNET_SAFETY_BUFFER as usize),
		);
	}
}

/// Generating the state machine-based electoral system
pub type AssethubDepositChannelWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<AssethubDepositChannelWitnessing>>;

// ------------------------ egress witnessing ---------------------------
// The electoral system for egress witnessing
define_empty_struct! { pub struct AssethubEgressWitnessing; }

impl BlockWitnesserInstance for AssethubEgressWitnessing {
	const BWNAME: &'static str = "Egress";
	type Runtime = Runtime;
	type Chain = AssethubChain;
	type BlockEntry = TransactionConfirmation<Runtime, AssethubInstance>;
	type ElectionProperties = Vec<PolkadotSignature>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, AssethubInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_broadcast::Config<AssethubInstance>>::SafeMode as Get<
			pallet_cf_broadcast::PalletSafeMode<AssethubInstance>,
		>>::get()
		.egress_witnessing_enabled
	}

	fn election_properties(
		_block_height: ChainBlockNumberOf<Self::Chain>,
	) -> Self::ElectionProperties {
		TransactionOutIdToBroadcastId::<Runtime, AssethubInstance>::iter()
			.map(|(tx_id, _)| tx_id)
			.collect::<Vec<_>>()
	}

	fn processed_up_to(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

/// Generating the state machine-based electoral system
pub type AssethubEgressWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<AssethubEgressWitnessing>>;

// ------------------------ liveness ---------------------------
pub type AssethubLiveness = Liveness<
	u64, /* we can't use the actual AssethubWitnessBatchNumber because that's based on u32 and
	      * the Liveness ES requires inter-convertibility with u64 due to the underlying
	      * randomness library. */
	sp_core::H256,
	ReportFailedLivenessCheck<Assethub>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ fee tracking ---------------------------
pub struct AssethubFeeUpdateHook;
impl UpdateFeeHook<AssethubTrackedData> for AssethubFeeUpdateHook {
	fn update_fee(fee: AssethubTrackedData) {
		if let Err(err) = AssethubChainTracking::inner_update_fee(fee.clone()) {
			log::error!("Failed to update hub fees to {fee:#?}: {err:?}");
		}
	}
}

pub type AssethubFeeTracking = UnsafeMedian<
	AssethubTrackedData,
	(),
	AssethubFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct AssethubElectionHooks;

impl
	Hooks<
		AssethubBlockHeightWitnesserES,
		AssethubDepositChannelWitnessingES,
		AssethubEgressWitnessingES,
		AssethubFeeTracking,
		AssethubLiveness,
	> for AssethubElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, egress_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<AssethubBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<AssethubDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<AssethubEgressWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<AssethubLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<AssethubFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let current_sc_block_number = crate::System::block_number();

		// Assethub witnesses finalized blocks and thus doesn't have a relevant safety margin
		// This means that we don't have to update the election-internal safety margin here
		// like we do for other chains.

		let chain_progress = AssethubBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				AssethubBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, AssethubInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

		AssethubDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				AssethubDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, AssethubInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress.clone())?;

		AssethubEgressWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				AssethubEgressWitnessingES,
				RunnerStorageAccess<Runtime, AssethubInstance>,
			>,
		>(egress_identifiers, &chain_progress.clone())?;

		AssethubFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				AssethubFeeTracking,
				RunnerStorageAccess<Runtime, AssethubInstance>,
			>,
		>(fee_identifiers, &current_sc_block_number)?;

		AssethubLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				AssethubLiveness,
				RunnerStorageAccess<Runtime, AssethubInstance>,
			>,
		>(
			liveness_identifiers,
			&(
				crate::System::block_number(),
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get()
					.unwrap()
					.block_height
					// We subtract the safety buffer so we don't ask for liveness for blocks that
					// could be reorged out.
					.saturating_sub(ASSETHUB_MAINNET_SAFETY_BUFFER)
					.into(),
				crate::Validator::current_epoch(),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, AssethubInstance> {
	InitialState {
		unsynchronised_state: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		),
		unsynchronised_settings: (
			BlockHeightWitnesserSettings { safety_buffer: ASSETHUB_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 0,
				safety_buffer: ASSETHUB_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 0,
				safety_buffer: ASSETHUB_MAINNET_SAFETY_BUFFER,
			},
			10, // open fee election every 10 SC blocks
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

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(
		<GenericBlockWitnesser<AssethubDepositChannelWitnessing> as BWTypes>::ElectionProperties,
	),
	Egresses(<GenericBlockWitnesser<AssethubEgressWitnessing> as BWTypes>::ElectionProperties),
}

pub struct ElectoralSystemConfiguration;
impl pallet_cf_elections::ElectoralSystemConfiguration for ElectoralSystemConfiguration {
	type SafeMode = ();

	type ElectoralEvents = AssethubElectoralEvents;

	type Properties = (<AssethubChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, AssethubInstance>::mutate_unsynchronised_state(
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
					log::error!("{e:?}: Failed to create deposit channel governance election with properties: {properties:?}");
				}
			},
			ElectionTypes::Egresses(egresses) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, AssethubInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _)| {
							state
								.2
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(egresses));
							Ok(())
						},
					) {
					log::error!(
						"{e:?}: Failed to create egress governance election with properties for block {block_height:?}"
					);
				},
		}
	}
}
