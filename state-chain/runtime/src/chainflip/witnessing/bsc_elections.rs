use core::ops::RangeInclusive;

use crate::{
	chainflip::{
		witnessing::pallet_hooks::{self, EvmKeyManagerEvent, EvmVaultContractEvent},
		ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	BscChainTracking, BscIngressEgress, Runtime,
};
use cf_chains::{
	bsc::BscTrackedData,
	evm,
	instances::BscInstance,
	witness_period::{BlockWitnessRange, SaturatingStep},
	Bsc, Chain, DepositChannel,
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
			tuple_6_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::state_machine_es::{
			StatemachineElectoralSystem, StatemachineElectoralSystemTypes,
		},
		unsafe_median::{UnsafeMedian, UpdateFeeHook},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, ElectoralSystemTypes, InitialState,
	InitialStateOf, RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

pub type BscElectoralSystemRunner = CompositeRunner<
	(
		BscBlockHeightWitnesserES,
		BscDepositChannelWitnessingES,
		BscVaultDepositWitnessingES,
		BscKeyManagerWitnessingES,
		BscFeeTracking,
		BscLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, BscInstance>,
	BscElectionHooks,
>;

define_empty_struct! { pub struct BscChain; }

impl ChainTypes for BscChain {
	type ChainBlockNumber = BlockWitnessRange<Bsc>;
	type ChainBlockHash = evm::H256;
	const NAME: &'static str = "Bsc";
}

pub const BSC_MAINNET_SAFETY_BUFFER: u32 = 8;

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo)]
pub enum BscElectoralEvents {
	ReorgDetected {
		reorged_blocks: RangeInclusive<<BscChain as ChainTypes>::ChainBlockNumber>,
	},
}

// ------------------------ block height tracking ---------------------------
// The electoral system for block height tracking

define_empty_struct! { pub struct BscBlockHeightWitnesser; }

impls! {
	for BscBlockHeightWitnesser:

	// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = BscChain;
		type ReorgHook = Self;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<BscChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<BscChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: <BscChain as ChainTypes>::ChainBlockNumber) {
			if let Err(err) = BscChainTracking::inner_update_chain_height(*block_height.root()) {
				log::error!("Failed to update BSC chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<<BscChain as ChainTypes>::ChainBlockNumber>) {
			pallet_cf_elections::Pallet::<Runtime, BscInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(BscElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

/// Generating the state machine-based electoral system
pub type BscBlockHeightWitnesserES = StatemachineElectoralSystem<BscBlockHeightWitnesser>;

// ------------------------ deposit channel witnessing ---------------------------
// The electoral system for deposit channel witnessing
define_empty_struct! { pub struct BscDepositChannelWitnessing; }

impl BlockWitnesserInstance for BscDepositChannelWitnessing {
	const BWNAME: &'static str = "DepositChannel";
	type Runtime = Runtime;
	type Chain = BscChain;
	type BlockEntry = DepositWitness<Bsc>;
	type ElectionProperties = Vec<DepositChannel<Bsc>>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BscInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<BscInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<BscInstance>,
		>>::get()
		.deposit_channel_witnessing_enabled
	}

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties {
		let height = height.root();
		BscIngressEgress::active_deposit_channels_at(
			// we advance by SAFETY_BUFFER before checking opened_at
			height.saturating_forward(BSC_MAINNET_SAFETY_BUFFER as usize),
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
		ProcessedUpTo::<Runtime, BscInstance>::set(
			up_to.root().saturating_backward(BSC_MAINNET_SAFETY_BUFFER as usize),
		);
	}
}

/// Generating the state machine-based electoral system
pub type BscDepositChannelWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<BscDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
// The electoral system for vault deposit witnessing
define_empty_struct! { pub struct BscVaultDepositWitnessing; }

impl BlockWitnesserInstance for BscVaultDepositWitnessing {
	const BWNAME: &'static str = "VaultDeposit";
	type Runtime = Runtime;
	type Chain = BscChain;
	type BlockEntry = EvmVaultContractEvent<Runtime, BscInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BscInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<BscInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<BscInstance>,
		>>::get()
		.vault_deposit_witnessing_enabled
	}

	fn election_properties(
		_block_height: pallet_cf_elections::electoral_systems::block_height_witnesser::ChainBlockNumberOf<Self::Chain>,
	) {
		// Vault address doesn't change, it is read by the engine on startup
	}

	fn processed_up_to(
		_block_height: pallet_cf_elections::electoral_systems::block_height_witnesser::ChainBlockNumberOf<Self::Chain>,
	) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

/// Generating the state machine-based electoral system
pub type BscVaultDepositWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<BscVaultDepositWitnessing>>;

// ------------------------ Key Manager witnessing ---------------------------
define_empty_struct! { pub struct BscKeyManagerWitnessing; }

impl BlockWitnesserInstance for BscKeyManagerWitnessing {
	const BWNAME: &'static str = "KeyManager";
	type Runtime = Runtime;
	type Chain = BscChain;
	type BlockEntry = EvmKeyManagerEvent<Runtime, BscInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BscInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_elections::Config<BscInstance>>::SafeMode as Get<
			BscElectionsSafeMode,
		>>::get()
		.key_manager_witnessing
	}

	fn election_properties(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// KeyManager address doesn't change, it is read by the engine on startup
	}

	fn processed_up_to(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

/// Generating the state machine-based electoral system
pub type BscKeyManagerWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<BscKeyManagerWitnessing>>;

// ------------------------ liveness ---------------------------
pub type BscLiveness = Liveness<
	<Bsc as Chain>::ChainBlockNumber,
	evm::H256,
	ReportFailedLivenessCheck<Bsc>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ fee tracking ---------------------------
pub struct BscFeeUpdateHook;
impl UpdateFeeHook<BscTrackedData> for BscFeeUpdateHook {
	fn update_fee(fee: BscTrackedData) {
		if let Err(err) = BscChainTracking::inner_update_fee(fee) {
			log::error!("Failed to update BSC fees to {fee:#?}: {err:?}");
		}
	}
}

pub type BscFeeTracking = UnsafeMedian<
	BscTrackedData,
	(),
	BscFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct BscElectionHooks;

impl
	Hooks<
		BscBlockHeightWitnesserES,
		BscDepositChannelWitnessingES,
		BscVaultDepositWitnessingES,
		BscKeyManagerWitnessingES,
		BscFeeTracking,
		BscLiveness,
	> for BscElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, key_manager_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<BscBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BscDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BscVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BscKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BscLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BscFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let current_sc_block_number = crate::System::block_number();

		let chain_progress = BscBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

		BscDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress.clone())?;

		BscVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(vault_deposits_identifiers, &chain_progress.clone())?;

		BscKeyManagerWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscKeyManagerWitnessingES,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(key_manager_identifiers, &chain_progress.clone())?;

		BscFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscFeeTracking,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(fee_identifiers, &current_sc_block_number)?;

		BscLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				BscLiveness,
				RunnerStorageAccess<Runtime, BscInstance>,
			>,
		>(
			liveness_identifiers,
			&(
				crate::System::block_number(),
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, BscInstance>::get()
					.unwrap()
					.block_height
					// We subtract the safety buffer so we don't ask for liveness for blocks that
					// could be reorged out.
					.saturating_sub(BSC_MAINNET_SAFETY_BUFFER.into()),
				crate::Validator::current_epoch(),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, BscInstance> {
	InitialState {
		unsynchronised_state: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		),
		unsynchronised_settings: (
			BlockHeightWitnesserSettings { safety_buffer: BSC_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: BSC_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: BSC_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: BSC_MAINNET_SAFETY_BUFFER,
			},
			Default::default(),
			(),
		),
		settings: (
			Default::default(),
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
	BscElectionsSafeMode;

	key_manager_witnessing,
}

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(<BscDepositChannelWitnessing as BlockWitnesserInstance>::ElectionProperties),
	Vaults(()),
	KeyManager(()),
}

pub struct ElectoralSystemConfiguration;
impl pallet_cf_elections::ElectoralSystemConfiguration for ElectoralSystemConfiguration {
	type SafeMode = BscElectionsSafeMode;

	type ElectoralEvents = BscElectoralEvents;

	type Properties = (<BscChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, BscInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _)| {
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
					RunnerStorageAccess::<Runtime, BscInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _)| {
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
					RunnerStorageAccess::<Runtime, BscInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _)| {
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
