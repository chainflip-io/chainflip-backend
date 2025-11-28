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
	instances::EthereumInstance,
	witness_period::SaturatingStep,
	Chain, DepositChannel, Ethereum,
};
use cf_traits::{impl_pallet_safe_mode, Chainflip};
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
use pallet_cf_funding::{EthTransactionHash, EthereumDepositAndSCCall, FlipBalance};
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
		EthereumScUtilsWitnessingES,
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
#[serde(bound(
	serialize = "VaultWitness: Serialize, <C as Chain>::ChainAmount: Serialize, <C as Chain>::ChainAccount: Serialize, <C as Chain>::ChainAsset: Serialize",
	deserialize = "VaultWitness: Deserialize<'de>, <C as Chain>::ChainAmount: Deserialize<'de>, <C as Chain>::ChainAccount: Deserialize<'de>, <C as Chain>::ChainAsset: Deserialize<'de>",
))]
#[scale_info(skip_type_params(VaultWitness, C))]
pub enum VaultEvents<VaultWitness, C: Chain> {
	SwapNativeFilter(VaultWitness),
	SwapTokenFilter(VaultWitness),
	XcallNativeFilter(VaultWitness),
	XcallTokenFilter(VaultWitness),
	TransferNativeFailedFilter {
		asset: <C as Chain>::ChainAsset,
		amount: <C as Chain>::ChainAmount,
		destination_address: <C as Chain>::ChainAccount,
	},
	TransferTokenFailedFilter {
		asset: <C as Chain>::ChainAsset,
		amount: <C as Chain>::ChainAmount,
		destination_address: <C as Chain>::ChainAccount,
	},
}

pub type EthereumVaultEvent = VaultEvents<VaultDepositWitness<Runtime, EthereumInstance>, Ethereum>;

pub(crate) type BlockDataVaultDeposit = Vec<EthereumVaultEvent>;

impls! {
	for TypesFor<EthereumVaultDepositWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataVaultDeposit;

		type Event = EthEvent<EthereumVaultEvent>;
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

	/// Vault address doesn't change, it is read by the engine on startup
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) {}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumVaultDepositWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumVaultDepositWitnessing>>;

// ------------------------ State Chain Gateway witnessing ---------------------------
pub struct EthereumStateChainGatewayWitnessing;

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
		type ElectionProperties = ();
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
			if <<Runtime as pallet_cf_elections::Config<EthereumInstance>>::SafeMode as Get<EthereumElectionsSafeMode>>::get()
			.state_chain_gateway_witnessing
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// StateChainGateway address doesn't change, it is read by the engine on startup
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) {}
	}
}

/// Generating the state machine-based electoral system
pub type EthereumStateChainGatewayWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumStateChainGatewayWitnessing>>;

// ------------------------ Key Manager witnessing ---------------------------
pub struct EthereumKeyManagerWitnessing;

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
#[scale_info(skip_type_params(
	AggKey,
	BlockNumber,
	TxInId,
	TxOutId,
	SignerId,
	TxFee,
	TxMetadata,
	TxRef
))]
#[allow(clippy::large_enum_variant)]
pub enum KeyManagerEvent<AggKey, BlockNumber, TxInId, TxOutId, SignerId, TxFee, TxMetadata, TxRef> {
	AggKeySetByGovKey {
		new_public_key: AggKey,
		block_number: BlockNumber,
		tx_id: TxInId,
	},
	SignatureAccepted {
		tx_out_id: TxOutId,
		signer_id: SignerId,
		tx_fee: TxFee,
		tx_metadata: TxMetadata,
		transaction_ref: TxRef,
	},
	GovernanceAction {
		call_hash: GovCallHash,
	},
}

pub type EthereumKeyManagerEvent = KeyManagerEvent<
	AggKeyFor<Runtime, EthereumInstance>,
	ChainBlockNumberFor<Runtime, EthereumInstance>,
	TransactionInIdFor<Runtime, EthereumInstance>,
	TransactionOutIdFor<Runtime, EthereumInstance>,
	SignerIdFor<Runtime, EthereumInstance>,
	TransactionFeeFor<Runtime, EthereumInstance>,
	TransactionMetadataFor<Runtime, EthereumInstance>,
	TransactionRefFor<Runtime, EthereumInstance>,
>;

pub(crate) type BlockDataKeyManager = Vec<EthereumKeyManagerEvent>;

impls! {
	for TypesFor<EthereumKeyManagerWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataKeyManager;

		type Event = EthEvent<EthereumKeyManagerEvent>;
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
			if <<Runtime as pallet_cf_elections::Config<EthereumInstance>>::SafeMode as Get<EthereumElectionsSafeMode>>::get()
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
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) { }
	}
}

/// Generating the state machine-based electoral system
pub type EthereumKeyManagerWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumKeyManagerWitnessing>>;

// ------------------------ SC Utils witnessing ---------------------------
pub struct EthereumScUtilsWitnessing;

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct ScUtilsCall {
	pub deposit_and_call: EthereumDepositAndSCCall,
	pub caller: <Ethereum as Chain>::ChainAccount,
	// use 0 padded ethereum address as account_id which the flip funds
	// are associated with on SC
	pub caller_account_id: AccountId,
	pub eth_tx_hash: EthTransactionHash,
}
pub(crate) type BlockDataScUtils = Vec<ScUtilsCall>;

impls! {
	for TypesFor<EthereumScUtilsWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = EthereumChain;

		type BlockData = BlockDataScUtils;

		type Event = EthEvent<ScUtilsCall>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;

		const BWNAME: &'static str = "ScUtils";
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
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataScUtils, Option<eth::H256>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_elections::Config<EthereumInstance>>::SafeMode as Get<EthereumElectionsSafeMode>>::get()
			.sc_utils_witnessing
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	/// ScUtils address doesn't change, it is read by the engine on startup
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: <Ethereum as Chain>::ChainBlockNumber) { }
	}
}

/// Generating the state machine-based electoral system
pub type EthereumScUtilsWitnessingES =
	StatemachineElectoralSystem<TypesFor<EthereumScUtilsWitnessing>>;

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

pub const FEE_HISTORY_WINDOW: u64 = 5;
pub const PRIORITY_FEE_PERCENTILE: u64 = 50;

/// Settings are FEE_HISTORY_WINDOW and PRIORITY_FEE_PERCENTILE (previously hardcoded in the engine)
pub type EthereumFeeTracking = UnsafeMedian<
	EthereumTrackedData,
	(u64, u64),
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
		EthereumScUtilsWitnessingES,
		EthereumFeeTracking,
		EthereumLiveness,
	> for EthereumElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, state_chain_gateway_identifiers, key_manager_identifiers, sc_utils_identifiers, fee_identifiers, liveness_identifiers): (
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
					<EthereumScUtilsWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
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
		let current_sc_block_number = crate::System::block_number();

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

		EthereumScUtilsWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumScUtilsWitnessingES,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(sc_utils_identifiers, &chain_progress.clone())?;

		EthereumFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				EthereumFeeTracking,
				RunnerStorageAccess<Runtime, EthereumInstance>,
			>,
		>(fee_identifiers, &current_sc_block_number)?;

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
				safety_margin: 3,
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
			(FEE_HISTORY_WINDOW, PRIORITY_FEE_PERCENTILE),
			LIVENESS_CHECK_DURATION,
		),
		shared_data_reference_lifetime: 8,
	}
}

impl_pallet_safe_mode! {
	EthereumElectionsSafeMode;

	state_chain_gateway_witnessing,
	key_manager_witnessing,
	sc_utils_witnessing
}

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(ElectionPropertiesDepositChannel),
	Vaults(()),
	StateChainGateway(()),
	KeyManager(()),
	ScUtils(()),
}

pub struct ElectoralSystemConfiguration;
impl pallet_cf_elections::ElectoralSystemConfiguration for ElectoralSystemConfiguration {
	type SafeMode = EthereumElectionsSafeMode;

	type ElectoralEvents = EthereumElectoralEvents;

	type Properties = (<EthereumChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, EthereumInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _, _, _)| {
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
					RunnerStorageAccess::<Runtime, EthereumInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _, _, _)| {
							state
								.2
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create vault witnessing governance election with properties for block {block_height}");
				},
			ElectionTypes::StateChainGateway(_) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, EthereumInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _, _, _)| {
							state
								.3
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create state chain gateway witnessing governance election with properties for block {block_height}");
				},
			ElectionTypes::KeyManager(_) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, EthereumInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _, _, _)| {
							state
								.4
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create key manager witnessing governance election with properties for block {block_height}");
				},
			ElectionTypes::ScUtils(_) =>
				if let Err(e) =
					RunnerStorageAccess::<Runtime, EthereumInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _, _, _)| {
							state
								.5
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(()));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create sc utils witnessing governance election with properties for block {block_height}");
				},
		}
	}
}
