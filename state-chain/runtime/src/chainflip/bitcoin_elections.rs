use crate::{
	chainflip::{
		address_derivation::btc::derive_current_and_previous_epoch_private_btc_vaults,
		ReportFailedLivenessCheck,
	},
	BitcoinChainTracking, BitcoinIngressEgress, Runtime,
};
use cf_chains::{
	btc::{
		self, deposit_address::DepositAddress, BitcoinFeeInfo, BitcoinTrackedData, BlockNumber,
		BtcAmount, Hash,
	},
	instances::BitcoinInstance,
	Bitcoin, Chain,
};
use cf_primitives::{AccountId, ChannelId};
use cf_runtime_utilities::log_or_panic;
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{TransactionConfirmation, TransactionOutIdToBroadcastId};
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus,
			state_machine::{BHWStateWrapper, BlockHeightTrackingSM, InputHeaders},
			BlockHeightChangeHook, ChainProgressFor, ChainTypes, HWTypes,
			HeightWitnesserProperties,
		},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{
				BWElectionProperties, BWProcessorTypes, BWStatemachine, BWTypes,
				BlockWitnesserSettings, BlockWitnesserState, ElectionPropertiesHook, HookTypeFor,
				SafeModeEnabledHook,
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
use pallet_cf_ingress_egress::{
	DepositChannelDetails, DepositWitness, PalletSafeMode, VaultDepositWitness,
};
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

const SAFETY_MARGIN: u32 = 8;

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct BitcoinBlockHeightTracking;

impls! {
	for TypesFor<BitcoinBlockHeightTracking>:

	ChainTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type ChainBlockHash = btc::Hash;
		const SAFETY_MARGIN: u32 = SAFETY_MARGIN;
	}
	/// Associating the SM related types to the struct
	HWTypes {
		const BLOCK_BUFFER_SIZE: usize = SAFETY_MARGIN as usize;
		type BlockHeightChangeHook = Self;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BHWStateWrapper<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = ();
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = HeightWitnesserProperties<Self>;
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<InputHeaders<Self>>;
		type Consensus = InputHeaders<Self>;
		type OnFinalizeContext = Vec<()>;
		type OnFinalizeReturn = Vec<ChainProgressFor<Self>>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ();
		type OnFinalizeReturnItem = ChainProgressFor<Self>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightTrackingConsensus<Self>;
		type Statemachine = BlockHeightTrackingSM<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: btc::BlockNumber) {
			if let Err(err) = BitcoinChainTracking::inner_update_chain_state(cf_chains::ChainState {
				block_height,
				tracked_data: BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(0) },
			}) {
				log::error!("Failed to update chain state: {:?}", err);
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

type ElectionPropertiesDepositChannel = Vec<DepositChannelDetails<Runtime, BitcoinInstance>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Bitcoin>>;

impls! {
	for TypesFor<BitcoinDepositChannelWitnessing>:

	ChainTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type ChainBlockHash = btc::Hash;

		const SAFETY_MARGIN: u32 = SAFETY_MARGIN;
	}

	/// Associating BW processor types
	BWProcessorTypes {
		type BlockData = BlockDataDepositChannel;

		type Event = BtcEvent<DepositWitness<Bitcoin>>;
		type Rules = Self;
		type Execute = Self;
		type LogEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesDepositChannel;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerEventHook = EmptyHook;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BlockWitnesserState<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = BlockWitnesserSettings;
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = BWElectionProperties<Self>;
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataDepositChannel, Option<btc::Hash>)>;
		type Consensus = (BlockDataDepositChannel, Option<btc::Hash>);
		type OnFinalizeContext = Vec<ChainProgressFor<Self>>;
		type OnFinalizeReturn = Vec<()>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgressFor<Self>;
		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
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

	/// implementation of reading deposit channels hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(
			&mut self,
			block_witness_root: btc::BlockNumber,
		) -> Vec<DepositChannelDetails<Runtime, BitcoinInstance>> {
			// TODO: Channel expiry
			BitcoinIngressEgress::active_deposit_channels_at(block_witness_root)
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

	ChainTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type ChainBlockHash = btc::Hash;

		const SAFETY_MARGIN: u32 = SAFETY_MARGIN;
	}

	/// Associating BW processor types
	BWProcessorTypes {
		type BlockData = BlockDataVaultDeposit;

		type Event = BtcEvent<VaultDepositWitness<Runtime, BitcoinInstance>>;
		type Rules = Self;
		type Execute = Self;

		type LogEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesVaultDeposit;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerEventHook = EmptyHook;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BlockWitnesserState<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = BlockWitnesserSettings;
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = BWElectionProperties<Self>;
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<(BlockDataVaultDeposit, Option<btc::Hash>)>;
		type Consensus = (BlockDataVaultDeposit, Option<btc::Hash>);
		type OnFinalizeContext = Vec<ChainProgressFor<Self>>;
		type OnFinalizeReturn = Vec<()>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgressFor<Self>;
		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
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

	ChainTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type ChainBlockHash = btc::Hash;

		const SAFETY_MARGIN: u32 = SAFETY_MARGIN;
	}

	/// Associating BW processor types
	BWProcessorTypes {
		type BlockData = EgressBlockData;

		type Event = BtcEvent<TransactionConfirmation<Runtime, BitcoinInstance>>;
		type Rules = Self;
		type Execute = Self;

		type LogEventHook = EmptyHook;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesEgressWitnessing;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
		type ElectionTrackerEventHook = EmptyHook;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BlockWitnesserState<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = BlockWitnesserSettings;
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = BWElectionProperties<Self>;
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<(EgressBlockData, Option<btc::Hash>)>;
		type Consensus = (EgressBlockData, Option<btc::Hash>);
		type OnFinalizeContext = Vec<ChainProgressFor<Self>>;
		type OnFinalizeReturn = Vec<()>;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgressFor<Self>;
		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStatemachine<Self>;
		type ConsensusMechanism = BWConsensus<Self>;
	}

	/// implementation of safe mode reading hook
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

	/// implementation of reading vault hook
	Hook<HookTypeFor<Self, ElectionPropertiesHook>> {
		fn run(&mut self, _block_witness_root: BlockNumber) -> Vec<Hash> {
			// we just loop through this, no need to know the block
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
		let current_sc_block_number = crate::System::block_number();

		log::info!("BitcoinElectionHooks::called");
		let chain_progress = BitcoinBlockHeightTrackingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinBlockHeightTrackingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(block_height_tracking_identifiers, &Vec::from([()]))?;

		log::info!("BitcoinElectionHooks::on_finalize: {:?}", chain_progress);
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

		let last_btc_block =
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();

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
			&(current_sc_block_number, last_btc_block.block_height.saturating_sub(3)),
		)?;

		Ok(())
	}
}

pub(crate) const LIVENESS_CHECK_DURATION: BlockNumberFor<Runtime> = 10;

// Channel expiry:
// We need to process elections in order, even after a safe mode pause. This is to ensure channel
// expiry is done correctly. During safe mode pause, we could get into a situation where the current
// state suggests that a channel is expired, but at the time of a previous block which we have not
// yet processed, the channel was not expired.
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
			// TODO: Write a migration to set this too.
			BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 3 },
			BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 3 },
			BlockWitnesserSettings { max_concurrent_elections: 15, safety_margin: 0 },
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
