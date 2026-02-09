use core::ops::RangeInclusive;

use crate::{
	chainflip::{
		witnessing::{
			bsc_block_processor::BscEvent,
			elections::TypesFor,
			ethereum_elections::{KeyManagerEvent, VaultEvents},
		},
		ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	BscChainTracking, BscIngressEgress, Runtime,
};
use cf_chains::{
	bsc::{self, BscTrackedData},
	instances::BscInstance,
	witness_period::{BlockWitnessRange, SaturatingStep},
	Bsc, Chain, DepositChannel,
};
use cf_traits::{hook_test_utils::EmptyHook, impl_pallet_safe_mode, Chainflip, Hook};
use cf_utilities::impls;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{
	SignerIdFor, TransactionFeeFor, TransactionMetadataFor, TransactionOutIdFor, TransactionRefFor,
};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::{
		block_height_witnesser::{
			consensus::BlockHeightWitnesserConsensus, primitives::NonemptyContinuousHeaders,
			state_machine::BlockHeightWitnesser, BHWTypes, BlockHeightChangeHook,
			BlockHeightWitnesserSettings, ChainProgress, ChainTypes, ReorgHook,
		},
		block_witnesser::{
			consensus::BWConsensus,
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
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo, VaultDepositWitness};
use pallet_cf_vaults::{AggKeyFor, ChainBlockNumberFor, TransactionInIdFor};
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

pub struct BscChainTag;
pub type BscChain = TypesFor<BscChainTag>;
impl ChainTypes for BscChain {
	type ChainBlockNumber = BlockWitnessRange<Bsc>;
	type ChainBlockHash = bsc::H256;
	const NAME: &'static str = "Bsc";
}

pub const BSC_MAINNET_SAFETY_BUFFER: u32 = 8;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum BscElectoralEvents {
	ReorgDetected { reorged_blocks: RangeInclusive<<BscChain as ChainTypes>::ChainBlockNumber> },
}

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct BscBlockHeightWitnesser;

impls! {
	for TypesFor<BscBlockHeightWitnesser>:

	/// Associating the SM related types to the struct
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
pub type BscBlockHeightWitnesserES = StatemachineElectoralSystem<TypesFor<BscBlockHeightWitnesser>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct BscDepositChannelWitnessing;

type ElectionPropertiesDepositChannel = Vec<DepositChannel<Bsc>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Bsc>>;

impls! {
	for TypesFor<BscDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BscChain;
		type BlockData = BlockDataDepositChannel;

		type Event = BscEvent<DepositWitness<Bsc>>;
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
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataDepositChannel, Option<bsc::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BscInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<BscInstance>,
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
			height: <BscChain as ChainTypes>::ChainBlockNumber,
		) -> Vec<DepositChannel<Bsc>> {
			let height = height.root();
			BscIngressEgress::active_deposit_channels_at(
				// we advance by SAFETY_BUFFER before checking opened_at
				height.saturating_forward(BSC_MAINNET_SAFETY_BUFFER as usize),
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
			up_to: <BscChain as ChainTypes>::ChainBlockNumber,
		) {
			// we go back SAFETY_BUFFER, such that we only actually expire once this amount of blocks have been additionally processed.
			ProcessedUpTo::<Runtime, BscInstance>::set(up_to.root().saturating_backward(BSC_MAINNET_SAFETY_BUFFER as usize));
		}
	}
}
/// Generating the state machine-based electoral system
pub type BscDepositChannelWitnessingES =
	StatemachineElectoralSystem<TypesFor<BscDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing
pub struct BscVaultDepositWitnessing;

pub type BscVaultEvent = VaultEvents<VaultDepositWitness<Runtime, BscInstance>, Bsc>;

pub(crate) type BlockDataVaultDeposit = Vec<BscVaultEvent>;

impls! {
	for TypesFor<BscVaultDepositWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BscChain;

		type BlockData = BlockDataVaultDeposit;

		type Event = BscEvent<BscVaultEvent>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "VaultDeposit";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ();
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataVaultDeposit, Option<bsc::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BscInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<BscInstance>,
			>>::get()
			.vault_deposit_witnessing_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// Vault address doesn't change, it is read by the engine on startup
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <BscChain as ChainTypes>::ChainBlockNumber) {}
	}
}

/// Generating the state machine-based electoral system
pub type BscVaultDepositWitnessingES =
	StatemachineElectoralSystem<TypesFor<BscVaultDepositWitnessing>>;

// ------------------------ Key Manager witnessing ---------------------------
pub struct BscKeyManagerWitnessing;

pub type BscKeyManagerEvent = KeyManagerEvent<
	AggKeyFor<Runtime, BscInstance>,
	ChainBlockNumberFor<Runtime, BscInstance>,
	TransactionInIdFor<Runtime, BscInstance>,
	TransactionOutIdFor<Runtime, BscInstance>,
	SignerIdFor<Runtime, BscInstance>,
	TransactionFeeFor<Runtime, BscInstance>,
	TransactionMetadataFor<Runtime, BscInstance>,
	TransactionRefFor<Runtime, BscInstance>,
>;

pub(crate) type BlockDataKeyManager = Vec<BscKeyManagerEvent>;

impls! {
	for TypesFor<BscKeyManagerWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BscChain;

		type BlockData = BlockDataKeyManager;

		type Event = BscEvent<BscKeyManagerEvent>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "KeyManager";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ();
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataKeyManager, Option<bsc::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_elections::Config<BscInstance>>::SafeMode as Get<BscElectionsSafeMode>>::get()
			.key_manager_witnessing
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// KeyManager address doesn't change, it is read by the engine on startup
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <BscChain as ChainTypes>::ChainBlockNumber) { }
	}
}

/// Generating the state machine-based electoral system
pub type BscKeyManagerWitnessingES = StatemachineElectoralSystem<TypesFor<BscKeyManagerWitnessing>>;

// ------------------------ liveness ---------------------------
pub type BscLiveness = Liveness<
	<Bsc as Chain>::ChainBlockNumber,
	bsc::H256,
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
			DerivedElectoralAccess<_, BscFeeTracking, RunnerStorageAccess<Runtime, BscInstance>>,
		>(fee_identifiers, &current_sc_block_number)?;

		BscLiveness::on_finalize::<
			DerivedElectoralAccess<_, BscLiveness, RunnerStorageAccess<Runtime, BscInstance>>,
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

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(ElectionPropertiesDepositChannel),
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
