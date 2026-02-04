use core::ops::RangeInclusive;

use crate::{
	chainflip::{
		witnessing::{
			arbitrum_block_processor::ArbEvent,
			elections::TypesFor,
			pallet_hooks::{self, EvmKeyManagerEvent, VaultContractEvent},
		},
		ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	ArbitrumChainTracking, ArbitrumIngressEgress, Runtime,
};
use cf_chains::{
	arb::{self, ArbitrumTrackedData},
	instances::ArbitrumInstance,
	witness_period::{BlockWitnessRange, SaturatingStep},
	Arbitrum, Chain, DepositChannel,
};
use cf_traits::{hook_test_utils::EmptyHook, impl_pallet_safe_mode, Chainflip, Hook};
use cf_utilities::impls;
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
			consensus::BWConsensus,
			instance::{BlockWitnesserInstance, GenericBlockWitnesser, JustWitnessAtSafetyMargin},
			primitives::SafeModeStatus,
			state_machine::{
				BWElectionType, BWProcessorTypes, BWStatemachine, BWTypes, BlockWitnesserSettings,
				ElectionPropertiesHook, HookTypeFor, ProcessedUpToHook, SafeModeEnabledHook,
			},
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

pub type ArbitrumElectoralSystemRunner = CompositeRunner<
	(
		ArbitrumBlockHeightWitnesserES,
		ArbitrumDepositChannelWitnessingES,
		ArbitrumVaultDepositWitnessingES,
		ArbitrumKeyManagerWitnessingES,
		ArbitrumFeeTracking,
		ArbitrumLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, ArbitrumInstance>,
	ArbitrumElectionHooks,
>;

pub struct ArbitrumChainTag;
pub type ArbitrumChain = TypesFor<ArbitrumChainTag>;
impl ChainTypes for ArbitrumChain {
	type ChainBlockNumber = BlockWitnessRange<Arbitrum>;
	type ChainBlockHash = arb::H256;
	const NAME: &'static str = "Arbitrum";
}

pub const ARBITRUM_MAINNET_SAFETY_BUFFER: u32 = 8;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum ArbitrumElectoralEvents {
	ReorgDetected {
		reorged_blocks: RangeInclusive<<ArbitrumChain as ChainTypes>::ChainBlockNumber>,
	},
}

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct ArbitrumBlockHeightWitnesser;

impls! {
	for TypesFor<ArbitrumBlockHeightWitnesser>:

	// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = ArbitrumChain;
		type ReorgHook = Self;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<ArbitrumChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<ArbitrumChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: <ArbitrumChain as ChainTypes>::ChainBlockNumber) {
			if let Err(err) = ArbitrumChainTracking::inner_update_chain_height(*block_height.root()) {
				log::error!("Failed to update arb chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<<ArbitrumChain as ChainTypes>::ChainBlockNumber>) {
			pallet_cf_elections::Pallet::<Runtime, ArbitrumInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(ArbitrumElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

/// Generating the state machine-based electoral system
pub type ArbitrumBlockHeightWitnesserES =
	StatemachineElectoralSystem<TypesFor<ArbitrumBlockHeightWitnesser>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct ArbitrumDepositChannelWitnessing;

type ElectionPropertiesDepositChannel = Vec<DepositChannel<Arbitrum>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Arbitrum>>;

impls! {
	for TypesFor<ArbitrumDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = ArbitrumChain;
		type BlockData = BlockDataDepositChannel;

		type Event = ArbEvent<DepositWitness<Arbitrum>>;
		type Rules = Self;
		type Execute = Self;
		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "DepositChannel";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesDepositChannel;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataDepositChannel, Option<arb::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<ArbitrumInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
			>>::get()
			.deposit_channel_witnessing_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// implementation of reading deposit channels hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(
			&mut self,
			height: <ArbitrumChain as ChainTypes>::ChainBlockNumber,
		) -> Vec<DepositChannel<Arbitrum>> {
			let height = height.root();
			ArbitrumIngressEgress::active_deposit_channels_at(
				// we advance by SAFETY_BUFFER before checking opened_at
				height.saturating_forward(ARBITRUM_MAINNET_SAFETY_BUFFER as usize),
				// we don't advance for expiry
				*height
			).into_iter().map(|deposit_channel_details| {
				deposit_channel_details.deposit_channel
			}).collect()
		}
	}

	/// implementation of processed_up_to hook, this enables expiration of deposit channels
	Hook<HookTypeFor<Self, ProcessedUpToHook>> {
		fn run(
			&mut self,
			up_to: <ArbitrumChain as ChainTypes>::ChainBlockNumber,
		) {
			// we go back SAFETY_BUFFER, such that we only actually expire once this amount of blocks have been additionally processed.
			ProcessedUpTo::<Runtime, ArbitrumInstance>::set(up_to.root().saturating_backward(ARBITRUM_MAINNET_SAFETY_BUFFER as usize));
		}
	}
}
/// Generating the state machine-based electoral system
pub type ArbitrumDepositChannelWitnessingES =
	StatemachineElectoralSystem<TypesFor<ArbitrumDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing
pub struct ArbitrumVaultDepositWitnessing;

impl BlockWitnesserInstance for TypesFor<ArbitrumVaultDepositWitnessing> {
	const BWNAME: &'static str = "VaultDeposit";
	type Runtime = Runtime;
	type Chain = ArbitrumChain;
	type BlockEntry = VaultContractEvent<Runtime, ArbitrumInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, ArbitrumInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<ArbitrumInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
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
pub type ArbitrumVaultDepositWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TypesFor<ArbitrumVaultDepositWitnessing>>>;

// ------------------------ Key Manager witnessing ---------------------------
pub struct ArbitrumKeyManagerWitnessing;

impl BlockWitnesserInstance for TypesFor<ArbitrumKeyManagerWitnessing> {
	const BWNAME: &'static str = "KeyManager";
	type Runtime = Runtime;
	type Chain = ArbitrumChain;
	type BlockEntry = EvmKeyManagerEvent<Runtime, ArbitrumInstance>;
	type ElectionProperties = ();
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, ArbitrumInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_elections::Config<ArbitrumInstance>>::SafeMode as Get<
			ArbitrumElectionsSafeMode,
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
pub type ArbitrumKeyManagerWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TypesFor<ArbitrumKeyManagerWitnessing>>>;

// ------------------------ liveness ---------------------------
pub type ArbitrumLiveness = Liveness<
	<Arbitrum as Chain>::ChainBlockNumber,
	arb::H256,
	ReportFailedLivenessCheck<Arbitrum>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ fee tracking ---------------------------
pub struct ArbitrumFeeUpdateHook;
impl UpdateFeeHook<ArbitrumTrackedData> for ArbitrumFeeUpdateHook {
	fn update_fee(fee: ArbitrumTrackedData) {
		if let Err(err) = ArbitrumChainTracking::inner_update_fee(fee) {
			log::error!("Failed to update arb fees to {fee:#?}: {err:?}");
		}
	}
}

pub type ArbitrumFeeTracking = UnsafeMedian<
	ArbitrumTrackedData,
	(),
	ArbitrumFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct ArbitrumElectionHooks;

impl
	Hooks<
		ArbitrumBlockHeightWitnesserES,
		ArbitrumDepositChannelWitnessingES,
		ArbitrumVaultDepositWitnessingES,
		ArbitrumKeyManagerWitnessingES,
		ArbitrumFeeTracking,
		ArbitrumLiveness,
	> for ArbitrumElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, key_manager_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<ArbitrumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<ArbitrumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<ArbitrumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<ArbitrumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<ArbitrumLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<ArbitrumFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let current_sc_block_number = crate::System::block_number();

		let chain_progress = ArbitrumBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

		ArbitrumDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress.clone())?;

		ArbitrumVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(vault_deposits_identifiers, &chain_progress.clone())?;

		ArbitrumKeyManagerWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumKeyManagerWitnessingES,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(key_manager_identifiers, &chain_progress.clone())?;

		ArbitrumFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumFeeTracking,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(fee_identifiers, &current_sc_block_number)?;

		ArbitrumLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				ArbitrumLiveness,
				RunnerStorageAccess<Runtime, ArbitrumInstance>,
			>,
		>(
			liveness_identifiers,
			&(
				crate::System::block_number(),
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get()
					.unwrap()
					.block_height
					// We subtract the safety buffer so we don't ask for liveness for blocks that
					// could be reorged out.
					.saturating_sub(ARBITRUM_MAINNET_SAFETY_BUFFER.into()),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, ArbitrumInstance> {
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
			BlockHeightWitnesserSettings { safety_buffer: ARBITRUM_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: ARBITRUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: ARBITRUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: ARBITRUM_MAINNET_SAFETY_BUFFER,
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
	ArbitrumElectionsSafeMode;

	key_manager_witnessing,
}

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(ElectionPropertiesDepositChannel),
	Vaults(()),
	KeyManager(()),
}

pub struct ElectoralSystemConfiguration;
impl pallet_cf_elections::ElectoralSystemConfiguration for ElectoralSystemConfiguration {
	type SafeMode = ArbitrumElectionsSafeMode;

	type ElectoralEvents = ArbitrumElectoralEvents;

	type Properties = (<ArbitrumChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, ArbitrumInstance>::mutate_unsynchronised_state(
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
					RunnerStorageAccess::<Runtime, ArbitrumInstance>::mutate_unsynchronised_state(
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
					RunnerStorageAccess::<Runtime, ArbitrumInstance>::mutate_unsynchronised_state(
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
