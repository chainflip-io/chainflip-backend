use crate::{
	chainflip::ReportFailedLivenessCheck, BitcoinChainTracking, BitcoinIngressEgress, Runtime,
};
use cf_chains::{
	btc::{self, BitcoinFeeInfo, BitcoinTrackedData, BlockNumber, Hash},
	instances::BitcoinInstance,
	Bitcoin,
};
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus,
			state_machine::{BHWStateWrapper, BlockHeightTrackingSM, InputHeaders},
			BlockHeightChangeHook, ChainProgress, HWTypes, HeightWitnesserProperties,
		},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{
				BWElectionProperties, BWProcessorTypes, BWStateMachine, BWTypes,
				BlockWitnesserSettings, BlockWitnesserState, ElectionPropertiesHook, HookTypeFor,
				SafeModeEnabledHook,
			},
		},
		composite::{
			tuple_3_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::{
			core::Hook,
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{
	DepositChannelDetails, DepositWitness, PalletSafeMode, ProcessedUpTo,
};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get, MaxEncodedLen};
use sp_std::vec::Vec;

use super::{bitcoin_block_processor::BtcEvent, elections::TypesFor};

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(BitcoinBlockHeightTrackingES, BitcoinDepositChannelWitnessingES, BitcoinLiveness),
	<Runtime as Chainflip>::ValidatorId,
	RunnerStorageAccess<Runtime, BitcoinInstance>,
	BitcoinElectionHooks,
>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OpenChannelDetails<ChainBlockNumber> {
	pub open_block: ChainBlockNumber,
	pub close_block: ChainBlockNumber,
}

// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct BitcoinBlockHeightTracking;

impls! {
	for TypesFor<BitcoinBlockHeightTracking>:

	/// Associating the SM related types to the struct
	HWTypes {
		const BLOCK_BUFFER_SIZE: usize = 6;
		type ChainBlockNumber = btc::BlockNumber;
		type ChainBlockHash = btc::Hash;
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
		type OnFinalizeReturn = Vec<ChainProgress<btc::BlockNumber>>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ();
		type OnFinalizeReturnItem = ChainProgress<btc::BlockNumber>;

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

type ElectionProperties = Vec<DepositChannelDetails<Runtime, BitcoinInstance>>;
pub(crate) type BlockData = Vec<DepositWitness<Bitcoin>>;

impls! {
	for TypesFor<BitcoinDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type BlockData = BlockData;

		type Event = BtcEvent;
		type Rules = Self;
		type Execute = Self;
		type DedupEvents = Self;
		type SafetyMargin = Self;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionProperties;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
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
		type VoteStorage = vote_storage::bitmap::Bitmap<BlockData>;
		type Consensus = BlockData;
		type OnFinalizeContext = Vec<ChainProgress<btc::BlockNumber>>;
		type OnFinalizeReturn = Vec<()>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgress<btc::BlockNumber>;
		type OnFinalizeReturnItem = ();

		// the actual state machine and consensus mechanisms of this ES
		type Statemachine = BWStateMachine<Self>;
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

pub type BitcoinLiveness = Liveness<
	BlockNumber,
	Hash,
	cf_primitives::BlockNumber,
	ReportFailedLivenessCheck<Bitcoin>,
	<Runtime as Chainflip>::ValidatorId,
>;

pub struct BitcoinElectionHooks;

impl Hooks<BitcoinBlockHeightTrackingES, BitcoinDepositChannelWitnessingES, BitcoinLiveness>
	for BitcoinElectionHooks
{
	fn on_finalize(
		(block_height_tracking_identifiers, deposit_channel_witnessing_identifiers, liveness_identifiers): (
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
		>(deposit_channel_witnessing_identifiers.clone(), &chain_progress)?;

		// We use `ProcessedUpTo` as our upper limit to avoid not reaching consensus in
		// case there is a reorg, using this block means safety margin will be kept into account for
		// this election, and thus are much less likely to ask nodes to query for a block they don't
		// have.
		let last_processed_block = ProcessedUpTo::<Runtime, BitcoinInstance>::get();
		BitcoinLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinLiveness,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(liveness_identifiers, &(current_sc_block_number, last_processed_block))?;

		Ok(())
	}
}

const LIVENESS_CHECK_DURATION: BlockNumberFor<Runtime> = 10;

// Channel expiry:
// We need to process elections in order, even after a safe mode pause. This is to ensure channel
// expiry is done correctly. During safe mode pause, we could get into a situation where the current
// state suggests that a channel is expired, but at the time of a previous block which we have not
// yet processed, the channel was not expired.
pub fn initial_state() -> InitialStateOf<Runtime, BitcoinInstance> {
	InitialState {
		unsynchronised_state: (Default::default(), Default::default(), Default::default()),
		unsynchronised_settings: (
			Default::default(),
			// TODO: Write a migration to set this too.
			BlockWitnesserSettings { max_concurrent_elections: 15 },
			(),
		),
		settings: (Default::default(), Default::default(), LIVENESS_CHECK_DURATION),
	}
}
