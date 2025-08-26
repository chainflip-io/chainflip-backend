use core::ops::RangeInclusive;

use crate::{
	chainflip::{
		elections::TypesFor, ethereum_block_processor::EthEvent, ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	AccountId, EthereumChainTracking, EthereumIngressEgress, Runtime,
};
use cf_chains::{
	eth::{self, EthereumTrackedData},
	evm::SchnorrVerificationComponents,
	instances::EthereumInstance,
	witness_period::SaturatingStep,
	Chain, DepositChannel, Ethereum,
};
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{
	SignerIdFor, TransactionConfirmation, TransactionFeeFor, TransactionMetadataFor,
	TransactionOutIdFor, TransactionOutIdToBroadcastId, TransactionRefFor,
};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
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
				BWProcessorTypes, BWStatemachine, BWTypes, BlockWitnesserSettings,
				ElectionPropertiesHook, HookTypeFor, ProcessedUpToHook, SafeModeEnabledHook,
			},
		},
		composite::{
			tuple_8_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::{
			core::{hook_test_utils::EmptyHook, Hook},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
		unsafe_median::{UnsafeMedian, UpdateFeeHook},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, ElectoralSystemTypes, InitialState,
	InitialStateOf, RunnerStorageAccess,
};
use pallet_cf_funding::{EthTransactionHash, FlipBalance};
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo, VaultDepositWitness};
use pallet_cf_vaults::{AggKeyFor, ChainBlockNumberFor, TransactionInIdFor};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{Decode, Encode, Get};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

pub type EthereumElectoralSystemRunner = CompositeRunner<
	(
		EthereumBlockHeightWitnesserES,
		EthereumDepositChannelWitnessingES,
		EthereumVaultDepositWitnessingES,
		EthereumStateChainGatewayWitnessingES,
		EthereumKeyManagerWitnessingES,
		EthereumEgressWitnessingES,
		EthereumFeeTracking,
		EthereumLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, EthereumInstance>,
	EthereumElectionHooks,
>;

pub struct EthereumChainTag;
pub type EthereumChain = TypesFor<EthereumChainTag>;
impl ChainTypes for EthereumChain {
	type ChainBlockNumber = <Ethereum as Chain>::ChainBlockNumber;
	type ChainBlockHash = eth::H256;
	const NAME: &'static str = "Ethereum";
}

pub const ETHEREUM_MAINNET_SAFETY_BUFFER: u32 = 8;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum EthereumElectoralEvents {
	ReorgDetected { reorged_blocks: RangeInclusive<<Ethereum as Chain>::ChainBlockNumber> },
}

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct EthereumBlockHeightWitnesser;

impls! {
	for TypesFor<EthereumBlockHeightWitnesser>:

	/// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = EthereumChain;
		type ReorgHook = Self;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<EthereumChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<EthereumChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: <Ethereum as Chain>::ChainBlockNumber) {
			if let Err(err) = EthereumChainTracking::inner_update_chain_height(block_height) {
				log::error!("Failed to update ETH chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<<Ethereum as Chain>::ChainBlockNumber>) {
			pallet_cf_elections::Pallet::<Runtime, EthereumInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(EthereumElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumBlockHeightWitnesserES =
	StatemachineElectoralSystem<TypesFor<EthereumBlockHeightWitnesser>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct EthereumDepositChannelWitnessing;

type ElectionPropertiesDepositChannel = Vec<DepositChannel<Ethereum>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Ethereum>>;

impls! {
	for TypesFor<EthereumDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;
		type BlockData = BlockDataDepositChannel;

		type Event = EthEvent<DepositWitness<Ethereum>>;
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
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataDepositChannel, Option<eth::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<EthereumInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
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
			height: <Ethereum as Chain>::ChainBlockNumber,
		) -> Vec<DepositChannel<Ethereum>> {

			EthereumIngressEgress::active_deposit_channels_at(
				// we advance by SAFETY_BUFFER before checking opened_at
				height.saturating_forward(ETHEREUM_MAINNET_SAFETY_BUFFER as usize),
				// we don't advance for expiry
				height
			).into_iter().map(|deposit_channel_details| {
				deposit_channel_details.deposit_channel
			}).collect()
		}
	}

	/// implementation of processed_up_to hook, this enables expiration of deposit channels
	Hook<HookTypeFor<Self, ProcessedUpToHook>> {
		fn run(
			&mut self,
			up_to: <Ethereum as Chain>::ChainBlockNumber,
		) {
			// we go back SAFETY_BUFFER, such that we only actually expire once this amount of blocks have been additionally processed.
			ProcessedUpTo::<Runtime, EthereumInstance>::set(up_to.saturating_backward(ETHEREUM_MAINNET_SAFETY_BUFFER as usize));
		}
	}
}
/// Generating the state machine-based electoral system
pub type EthereumDepositChannelWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing
pub struct EthereumVaultDepositWitnessing;

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum VaultEvents {
	SwapNativeFilter(VaultDepositWitness<Runtime, EthereumInstance>),
	SwapTokenFilter(VaultDepositWitness<Runtime, EthereumInstance>),
	XcallNativeFilter(VaultDepositWitness<Runtime, EthereumInstance>),
	XcallTokenFilter(VaultDepositWitness<Runtime, EthereumInstance>),
	TransferNativeFailedFilter {
		asset: cf_chains::assets::eth::Asset,
		amount: <Ethereum as Chain>::ChainAmount,
		destination_address: <Ethereum as Chain>::ChainAccount,
	},
	TransferTokenFailedFilter {
		asset: cf_chains::assets::eth::Asset,
		amount: <Ethereum as Chain>::ChainAmount,
		destination_address: <Ethereum as Chain>::ChainAccount,
	},
}

type ElectionPropertiesVaultDeposit = <Ethereum as Chain>::ChainAccount;
pub(crate) type BlockDataVaultDeposit = Vec<VaultEvents>;

impls! {
	for TypesFor<EthereumVaultDepositWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataVaultDeposit;

		type Event = EthEvent<VaultEvents>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "VaultDeposit";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesVaultDeposit;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataVaultDeposit, Option<eth::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<EthereumInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
			>>::get()
			.vault_deposit_witnessing_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// implementation of reading vault hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) -> ElectionPropertiesVaultDeposit {
			pallet_cf_environment::EthereumVaultAddress::<Runtime>::get()
		}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumVaultDepositWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumVaultDepositWitnessing>>;

// ------------------------ State Chain Gateway witnessing ---------------------------
pub struct EthereumStateChainGatewayWitnessing;

type ElectionPropertiesStateChainGateway = <Ethereum as Chain>::ChainAccount;
#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub enum StateChainGatewayEvent {
	Funded {
		account_id: AccountId,
		amount: FlipBalance<Runtime>,
		funder: eth::Address,
		tx_hash: EthTransactionHash,
	},
	RedemptionExecuted {
		account_id: AccountId,
		redeemed_amount: FlipBalance<Runtime>,
		tx_hash: EthTransactionHash,
	},
	RedemptionExpired {
		account_id: AccountId,
		block_number: u64,
	},
}
pub(crate) type BlockDataStateChainGateway = Vec<StateChainGatewayEvent>;

impls! {
	for TypesFor<EthereumStateChainGatewayWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataStateChainGateway;

		type Event = EthEvent<StateChainGatewayEvent>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "StateChainGateway";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesStateChainGateway;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataStateChainGateway, Option<eth::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			//TODO: do we want to add separate safe modes?
				SafeModeStatus::Disabled
		}
	}

	/// implementation of reading vault hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) -> ElectionPropertiesVaultDeposit {
			pallet_cf_environment::EthereumStateChainGatewayAddress::<Runtime>::get()
		}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumStateChainGatewayWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumStateChainGatewayWitnessing>>;

// ------------------------ Key Manager witnessing ---------------------------
pub struct EthereumKeyManagerWitnessing;

type ElectionPropertiesKeyManager = <Ethereum as Chain>::ChainAccount;
#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
#[allow(clippy::large_enum_variant)]
pub enum KeyManagerEvent {
	AggKeySetByGovKey {
		new_public_key: AggKeyFor<Runtime, EthereumInstance>,
		block_number: ChainBlockNumberFor<Runtime, EthereumInstance>,
		tx_id: TransactionInIdFor<Runtime, EthereumInstance>,
	},
	SignatureAccepted {
		tx_out_id: TransactionOutIdFor<Runtime, EthereumInstance>,
		signer_id: SignerIdFor<Runtime, EthereumInstance>,
		tx_fee: TransactionFeeFor<Runtime, EthereumInstance>,
		tx_metadata: TransactionMetadataFor<Runtime, EthereumInstance>,
		transaction_ref: TransactionRefFor<Runtime, EthereumInstance>,
	},
	GovernanceAction {
		call_hash: GovCallHash,
	},
}
pub(crate) type BlockDataKeyManager = Vec<KeyManagerEvent>;

impls! {
	for TypesFor<EthereumKeyManagerWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataKeyManager;

		type Event = EthEvent<KeyManagerEvent>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "KeyManager";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesKeyManager;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataKeyManager, Option<eth::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			//TODO: do we want to add separate safe modes?
				SafeModeStatus::Disabled
		}
	}

	/// implementation of reading vault hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) -> ElectionPropertiesKeyManager {
			pallet_cf_environment::EthereumKeyManagerAddress::<Runtime>::get()
		}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumKeyManagerWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumKeyManagerWitnessing>>;

// ------------------------ egress witnessing ---------------------------
/// The electoral system for egress witnessing
pub struct EthereumEgressWitnessing;

type ElectionPropertiesEgressWitnessing = Vec<SchnorrVerificationComponents>;

pub(crate) type EgressBlockData = Vec<TransactionConfirmation<Runtime, EthereumInstance>>;

impls! {
	for TypesFor<EthereumEgressWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;
		type BlockData = EgressBlockData;

		type Event = EthEvent<TransactionConfirmation<Runtime, EthereumInstance>>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "Egress";
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesEgressWitnessing;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ProcessedUpToHook = EmptyHook;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(EgressBlockData, Option<eth::H256>)>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_broadcast::Config<EthereumInstance>>::SafeMode as Get<
				pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
			>>::get()
			.egress_witnessing_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// implementation of reading vault hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) -> Vec<SchnorrVerificationComponents> {
			TransactionOutIdToBroadcastId::<Runtime, EthereumInstance>::iter()
				.map(|(tx_id, _)| tx_id)
				.collect::<Vec<_>>()
		}
	}

}

/// Generating the state machine-based electoral system
pub type EthereumEgressWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumEgressWitnessing>>;

// ------------------------ liveness ---------------------------
pub type EthereumLiveness = Liveness<
	<Ethereum as Chain>::ChainBlockNumber,
	eth::H256,
	ReportFailedLivenessCheck<Ethereum>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ fee tracking ---------------------------
pub struct EthereumFeeUpdateHook;
impl UpdateFeeHook<EthereumTrackedData> for EthereumFeeUpdateHook {
	fn update_fee(fee: EthereumTrackedData) {
		if let Err(err) = EthereumChainTracking::inner_update_fee(fee) {
			log::error!("Failed to update ETH fees to {fee:#?}: {err:?}");
		}
	}
}

// Ethereum fees are divided into base_fee and priority_fee, we can't directly use UnsafeMedian as
// it is -> EDIT: yes we can we just need to ensure that EthereumTrackedData impl Ord correctly such
// that the fees are ordered as we want and we take the correct median
/// TODO: MANUALLY IMPLEMENT ORD FOR EthereumTrackedData!!!
///
/// Possibly introduce some settings like FEE_HISTORY_WINDOW and PRIORITY_FEE_PERCENTILE which are
/// now hardcoded in the engine
pub type EthereumFeeTracking = UnsafeMedian<
	EthereumTrackedData,
	EthereumTrackedData,
	(),
	EthereumFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct EthereumElectionHooks;

impl
	Hooks<
		EthereumBlockHeightWitnesserES,
		EthereumDepositChannelWitnessingES,
		EthereumVaultDepositWitnessingES,
		EthereumStateChainGatewayWitnessingES,
		EthereumKeyManagerWitnessingES,
		EthereumEgressWitnessingES,
		EthereumFeeTracking,
		EthereumLiveness,
	> for EthereumElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, state_chain_gateway_identifiers, key_manager_identifiers, egress_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumStateChainGatewayWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumEgressWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<EthereumFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let chain_progress = EthereumBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

		EthereumDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress.clone())?;

		EthereumVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(vault_deposits_identifiers, &chain_progress.clone())?;

		EthereumStateChainGatewayWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumStateChainGatewayWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(state_chain_gateway_identifiers, &chain_progress.clone())?;

		EthereumKeyManagerWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumKeyManagerWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(key_manager_identifiers, &chain_progress.clone())?;

		EthereumEgressWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumEgressWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(egress_identifiers, &chain_progress.clone())?;

		EthereumFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumFeeTracking,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(fee_identifiers, &())?;

		EthereumLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumLiveness,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(
			liveness_identifiers,
			&(
				crate::System::block_number(),
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get()
					.unwrap()
					.block_height
					// We subtract the safety buffer so we don't ask for liveness for blocks that
					// could be reorged out.
					.saturating_sub(ETHEREUM_MAINNET_SAFETY_BUFFER.into()),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, EthereumInstance> {
	InitialState {
		unsynchronised_state: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		),
		unsynchronised_settings: (
			BlockHeightWitnesserSettings { safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
				safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
				safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
				safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
				safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 0,
				safety_buffer: ETHEREUM_MAINNET_SAFETY_BUFFER,
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
			Default::default(),
			Default::default(),
			LIVENESS_CHECK_DURATION,
		),
		shared_data_reference_lifetime: 8,
	}
}

pub struct EthereumGovernanceElectionHook;
impl pallet_cf_elections::GovernanceElectionHook for EthereumGovernanceElectionHook {
	type Properties = ();

	fn start(_properties: Self::Properties) {
		todo!()
	}
}
