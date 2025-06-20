use crate::{
	chainflip::{
		address_derivation::btc::derive_current_and_previous_epoch_private_btc_vaults,
		ReportFailedLivenessCheck,
	},
	constants::common::LIVENESS_CHECK_DURATION,
	BitcoinChainTracking, BitcoinIngressEgress, Runtime,
};
use cf_chains::{
	btc::{
		self, deposit_address::DepositAddress, BitcoinFeeInfo, BitcoinTrackedData, BlockNumber,
		BtcAmount, Hash,
	},
	instances::BitcoinInstance,
	Bitcoin, Chain, DepositChannel,
};
use cf_primitives::{AccountId, ChannelId};
use cf_runtime_utilities::log_or_panic;
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{TransactionConfirmation, TransactionOutIdToBroadcastId};
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus, primitives::NonemptyContinuousHeaders,
			state_machine::BlockHeightWitnesser, BHWTypes, BlockHeightChangeHook, ChainProgress,
			ChainTypes,
		},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{
				BWElectionType, BWProcessorTypes, BWStatemachine, BWTypes, BlockWitnesserSettings,
				ElectionPropertiesHook, HookTypeFor, SafeModeEnabledHook,
			},
		},
		composite::{
			tuple_6_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::{
			core::{hook_test_utils::EmptyHook, Hook},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
		unsafe_median::{UnsafeMedian, UpdateFeeHook},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get, MaxEncodedLen};
use sp_std::vec::Vec;

use super::{bitcoin_block_processor::BtcEvent, elections::TypesFor};

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(
		BitcoinBlockHeightTrackingES,
		BitcoinDepositChannelWitnessingES,
		BitcoinVaultDepositWitnessingES,
		BitcoinEgressWitnessingES,
		BitcoinFeeTracking,
		BitcoinLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, BitcoinInstance>,
	BitcoinElectionHooks,
>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OpenChannelDetails<ChainBlockNumber> {
	pub open_block: ChainBlockNumber,
	pub close_block: ChainBlockNumber,
}

const SAFETY_BUFFER: usize = 8;

pub struct BitcoinChainTag;
pub type BitcoinChain = TypesFor<BitcoinChainTag>;
impl ChainTypes for BitcoinChain {
	type ChainBlockNumber = btc::BlockNumber;
	type ChainBlockHash = btc::Hash;
	const SAFETY_BUFFER: usize = SAFETY_BUFFER;
}

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct BitcoinBlockHeightTracking;

impls! {
	for TypesFor<BitcoinBlockHeightTracking>:

	/// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = BitcoinChain;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<BitcoinChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<BitcoinChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightTrackingConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: btc::BlockNumber) {
			if let Err(err) = BitcoinChainTracking::inner_update_chain_height(block_height) {
				log::error!("Failed to update BTC chain height to {block_height:?}: {:?}", err);
			}
		}
	}

}

/// Generating the state machine-based electoral system
pub type BitcoinBlockHeightTrackingES =
	StatemachineElectoralSystem<TypesFor<BitcoinBlockHeightTracking>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct BitcoinDepositChannelWitnessing;

type ElectionPropertiesDepositChannel = Vec<DepositChannel<Bitcoin>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Bitcoin>>;

impls! {
	for TypesFor<BitcoinDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BitcoinChain;
		type BlockData = BlockDataDepositChannel;

		type Event = BtcEvent<DepositWitness<Bitcoin>>;
		type Rules = Self;
		type Execute = Self;
		type DebugEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesDepositChannel;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataDepositChannel, Option<btc::Hash>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<(), SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
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
			block_witness_root: btc::BlockNumber,
		) -> Vec<DepositChannel<Bitcoin>> {
			// TODO: Channel expiry
			BitcoinIngressEgress::active_deposit_channels_at(block_witness_root).into_iter().map(|deposit_channel_details| {
				deposit_channel_details.deposit_channel
			}).collect()
		}
	}

	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
				PalletSafeMode<BitcoinInstance>,
			>>::get()
			.deposits_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}
}
/// Generating the state machine-based electoral system
pub type BitcoinDepositChannelWitnessingES =
	StatemachineElectoralSystem<TypesFor<BitcoinDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing
pub struct BitcoinVaultDepositWitnessing;

type ElectionPropertiesVaultDeposit = Vec<(DepositAddress, AccountId, ChannelId)>;
pub(crate) type BlockDataVaultDeposit = Vec<VaultDepositWitness<Runtime, BitcoinInstance>>;

impls! {
	for TypesFor<BitcoinVaultDepositWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BitcoinChain;

		type BlockData = BlockDataVaultDeposit;

		type Event = BtcEvent<VaultDepositWitness<Runtime, BitcoinInstance>>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesVaultDeposit;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataVaultDeposit, Option<btc::Hash>)>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<(), SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
				pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
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
		fn run(&mut self, _block_witness_root: BlockNumber) -> ElectionPropertiesVaultDeposit {
			pallet_cf_swapping::BrokerPrivateBtcChannels::<Runtime>::iter()
				.flat_map(|(broker_id, channel_id)| {
					derive_current_and_previous_epoch_private_btc_vaults(channel_id)
						.map_err(|err| {
							log_or_panic!("Error while deriving private BTC addresses: {err:#?}")
						})
						.ok()
						.into_iter()
						.flatten()
						.map(move |address| (address, broker_id.clone(), channel_id))
				})
				.collect::<Vec<_>>()
		}
	}

	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
				PalletSafeMode<BitcoinInstance>,
			>>::get()
			.deposits_enabled
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

}

/// Generating the state machine-based electoral system
pub type BitcoinVaultDepositWitnessingES =
	StatemachineElectoralSystem<TypesFor<BitcoinVaultDepositWitnessing>>;

// ------------------------ egress witnessing ---------------------------
/// The electoral system for egress witnessing
pub struct BitcoinEgressWitnessing;

type ElectionPropertiesEgressWitnessing = Vec<Hash>;

pub(crate) type EgressBlockData = Vec<TransactionConfirmation<Runtime, BitcoinInstance>>;

impls! {
	for TypesFor<BitcoinEgressWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type Chain = BitcoinChain;
		type BlockData = EgressBlockData;

		type Event = BtcEvent<TransactionConfirmation<Runtime, BitcoinInstance>>;
		type Rules = Self;
		type Execute = Self;

		type DebugEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesEgressWitnessing;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerDebugEventHook = EmptyHook;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<(EgressBlockData, Option<btc::Hash>)>;

		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_broadcast::Config<BitcoinInstance>>::SafeMode as Get<
				pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
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
		fn run(&mut self, _block_witness_root: BlockNumber) -> Vec<Hash> {
			TransactionOutIdToBroadcastId::<Runtime, BitcoinInstance>::iter()
				.map(|(tx_id, _)| tx_id)
				.collect::<Vec<_>>()
		}
	}

}

/// Generating the state machine-based electoral system
pub type BitcoinEgressWitnessingES = StatemachineElectoralSystem<TypesFor<BitcoinEgressWitnessing>>;
pub type BitcoinLiveness = Liveness<
	BlockNumber,
	Hash,
	cf_primitives::BlockNumber,
	ReportFailedLivenessCheck<Bitcoin>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct BitcoinFeeUpdateHook;
impl UpdateFeeHook<BtcAmount> for BitcoinFeeUpdateHook {
	fn update_fee(fee: BtcAmount) {
		if let Err(err) = BitcoinChainTracking::inner_update_fee(BitcoinTrackedData {
			btc_fee_info: BitcoinFeeInfo::new(fee),
		}) {
			log::error!("Failed to update BTC fees to {fee:#?}: {err:?}");
		}
	}
}
pub type BitcoinFeeTracking = UnsafeMedian<
	<Bitcoin as Chain>::ChainAmount,
	BtcAmount,
	(),
	BitcoinFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub struct BitcoinElectionHooks;

impl
	Hooks<
		BitcoinBlockHeightTrackingES,
		BitcoinDepositChannelWitnessingES,
		BitcoinVaultDepositWitnessingES,
		BitcoinEgressWitnessingES,
		BitcoinFeeTracking,
		BitcoinLiveness,
	> for BitcoinElectionHooks
{
	fn on_finalize(
		(block_height_tracking_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, egress_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<BitcoinBlockHeightTrackingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let chain_progress = BitcoinBlockHeightTrackingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinBlockHeightTrackingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(block_height_tracking_identifiers, &Vec::from([()]))?;

		BitcoinDepositChannelWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinDepositChannelWitnessingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(deposit_channel_witnessing_identifiers.clone(), &chain_progress.clone())?;

		BitcoinVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(vault_deposits_identifiers.clone(), &chain_progress.clone())?;

		BitcoinEgressWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinEgressWitnessingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(egress_identifiers, &chain_progress.clone())?;

		BitcoinFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinFeeTracking,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(fee_identifiers, &())?;

		BitcoinLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinLiveness,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(
			liveness_identifiers,
			&(
				crate::System::block_number(),
				pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get()
					.unwrap()
					.block_height
					// We subtract the safety buffer so we don't ask for liveness for blocks that
					// could be reorged out.
					.saturating_sub(
						BitcoinChain::SAFETY_BUFFER
							.try_into()
							.map_err(|_| CorruptStorageError::new())?,
					),
			),
		)?;

		Ok(())
	}
}

pub fn initial_state() -> InitialStateOf<Runtime, BitcoinInstance> {
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
			Default::default(),
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 3,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 0,
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

pub struct BitcoinGovernanceElectionHook;

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(ElectionPropertiesDepositChannel),
	Vaults(ElectionPropertiesVaultDeposit),
	Egresses(ElectionPropertiesEgressWitnessing),
}

impl pallet_cf_elections::GovernanceElectionHook for BitcoinGovernanceElectionHook {
	type Properties = (<BitcoinChain as ChainTypes>::ChainBlockNumber, ElectionTypes);

	fn start(properties: Self::Properties) {
		let (block_height, election_type) = properties.clone();
		match election_type {
			ElectionTypes::DepositChannels(channels) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, BitcoinInstance>::mutate_unsynchronised_state(
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
			ElectionTypes::Vaults(vaults) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, BitcoinInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _)| {
							state
								.2
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(vaults));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create governance election with properties: {properties:?}");
				}
			},
			ElectionTypes::Egresses(egresses) => {
				if let Err(e) =
					RunnerStorageAccess::<Runtime, BitcoinInstance>::mutate_unsynchronised_state(
						|state: &mut (_, _, _, _, _, _)| {
							state
								.3
								.elections
								.ongoing
								.entry(block_height)
								.or_insert(BWElectionType::Governance(egresses));
							Ok(())
						},
					) {
					log::error!("{e:?}: Failed to create governance election with properties: {properties:?}");
				}
			},
		}
	}
}
