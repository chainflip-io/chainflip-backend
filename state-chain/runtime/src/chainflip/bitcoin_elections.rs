use crate::{BitcoinChainTracking, BitcoinIngressEgress, Runtime};
use cf_chains::{
	btc::{self, BitcoinFeeInfo, BitcoinTrackedData},
	instances::BitcoinInstance,
	Bitcoin,
};
use cf_traits::Chainflip;
use core::ops::RangeInclusive;
use frame_support::__private::sp_tracing::event;
use frame_system::pallet_prelude::BlockNumberFor;
use log::warn;
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus,
			state_machine::{BHWStateWrapper, BlockHeightTrackingSM, InputHeaders},
			BlockHeightTrackingProperties, BlockHeightTrackingTypes, ChainProgress,
		},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{BWSettings, BWState, BWStateMachine, BWTypes},
		},
		composite::{
			tuple_3_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		state_machine::{
			core::{ConstantIndex, Hook},
			state_machine_es::{StateMachineES, StateMachineESInstance},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use serde::{Deserialize, Serialize};
use sp_core::{Decode, Encode, Get, MaxEncodedLen};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
};

use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness, PalletSafeMode};
use scale_info::TypeInfo;

use crate::{
	chainflip::{
		bitcoin_block_processor::{
			BlockWitnessingProcessorDefinition, BtcEvent, DepositChannelWitnessingProcessor,
		},
		Offence, ReportFailedLivenessCheck,
	},
	Reputation,
};
use cf_chains::btc::{BlockNumber, Hash};
use cf_primitives::ForeignChain;
use cf_traits::offence_reporting::OffenceReporter;
use pallet_cf_elections::electoral_systems::{
	block_witnesser::{primitives::ChainProgressInner, state_machine::BWProcessorTypes},
	liveness::{Liveness, OnCheckComplete},
	state_machine::{
		core::{IndexOf, Indexed, Validate},
		state_machine::StateMachine,
	},
};
use sp_std::{vec, vec::Vec};
use crate::chainflip::bitcoin_block_processor::BlockWitnessingProcessorDefinition;

use sp_std::vec::Vec;

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(BitcoinBlockHeightTracking, BitcoinDepositChannelWitnessing, BitcoinLiveness),
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
#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Serialize,
	Deserialize,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct BitcoinBlockHeightTrackingTypes {}

/// Associating the SM related types to the struct
impl BlockHeightTrackingTypes for BitcoinBlockHeightTrackingTypes {
	const BLOCK_BUFFER_SIZE: usize = 6;
	type ChainBlockNumber = btc::BlockNumber;
	type ChainBlockHash = btc::Hash;
	type BlockHeightChangeHook = BitcoinBlockHeightChangeHook;
}

/// Associating the ES related types to the struct
impl ElectoralSystemTypes for BitcoinBlockHeightTrackingTypes {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState = BHWStateWrapper<BitcoinBlockHeightTrackingTypes>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = BlockHeightTrackingProperties<btc::BlockNumber>;
	type ElectionState = ();
	type VoteStorage = vote_storage::bitmap::Bitmap<InputHeaders<BitcoinBlockHeightTrackingTypes>>;
	type Consensus = InputHeaders<BitcoinBlockHeightTrackingTypes>;
	type OnFinalizeContext = Vec<()>;
	type OnFinalizeReturn = Vec<ChainProgress<btc::BlockNumber>>;
}

/// Associating the state machine and consensus mechanism to the struct
impl StateMachineES for BitcoinBlockHeightTrackingTypes {
	// both context and return have to be vectors, these are the item types
	type OnFinalizeContextItem = ();
	type OnFinalizeReturnItem = ChainProgress<btc::BlockNumber>;

	// restating types since we have to prove that they have the correct bounds
	type Consensus2 = InputHeaders<BitcoinBlockHeightTrackingTypes>;
	type Vote2 = InputHeaders<BitcoinBlockHeightTrackingTypes>;
	type VoteStorage2 = vote_storage::bitmap::Bitmap<InputHeaders<BitcoinBlockHeightTrackingTypes>>;

	// the actual state machine and consensus mechanisms of this ES
	type ConsensusMechanism = BlockHeightTrackingConsensus<BitcoinBlockHeightTrackingTypes>;
	type StateMachine = BlockHeightTrackingSM<BitcoinBlockHeightTrackingTypes>;
}

/// Hooks
#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct BitcoinBlockHeightChangeHook {}

impl Hook<btc::BlockNumber, ()> for BitcoinBlockHeightChangeHook {
	fn run(&mut self, block_height: btc::BlockNumber) {
		if let Err(err) = BitcoinChainTracking::inner_update_chain_state(cf_chains::ChainState {
			block_height,
			tracked_data: BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(0) },
		}) {
			log::error!("Failed to update chain state: {:?}", err);
		}
	}
}

/// Generating the state machine-based electoral system
pub type BitcoinBlockHeightTracking = StateMachineESInstance<BitcoinBlockHeightTrackingTypes>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct BitcoinDepositChannelWitnessingDefinition {}

type ElectionProperties = Vec<DepositChannelDetails<Runtime, BitcoinInstance>>;
pub(crate) type BlockData = Vec<DepositWitness<Bitcoin>>;

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct BitcoinSafemodeEnabledHook {}

impl Hook<(), SafeModeStatus> for BitcoinSafemodeEnabledHook {
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

/// Associating BW types to the struct
impl BWTypes for BitcoinDepositChannelWitnessingDefinition {
	type ChainBlockNumber = btc::BlockNumber;
	type BlockData = BlockData;
	type ElectionProperties = ElectionProperties;
	type ElectionPropertiesHook = BitcoinDepositChannelWitnessingGenerator;
	type SafeModeEnabledHook = BitcoinSafemodeEnabledHook;
	type BWProcessorTypes = BlockWitnessingProcessorDefinition;
}

/// Associating the ES related types to the struct
impl ElectoralSystemTypes for BitcoinDepositChannelWitnessingDefinition {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState = BWState<BitcoinDepositChannelWitnessingDefinition>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = BWSettings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (btc::BlockNumber, ElectionProperties, u8);
	type ElectionState = ();
	type VoteStorage = vote_storage::bitmap::Bitmap<
		ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>,
	>;
	type Consensus = ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type OnFinalizeContext = Vec<ChainProgress<btc::BlockNumber>>;
	type OnFinalizeReturn = Vec<()>;
}

/// Associating the state machine and consensus mechanism to the struct
impl StateMachineES for BitcoinDepositChannelWitnessingDefinition {
	// both context and return have to be vectors, these are the item types
	type OnFinalizeContextItem = ChainProgress<btc::BlockNumber>;
	type OnFinalizeReturnItem = ();

	// restating types since we have to prove that they have the correct bounds
	type Consensus2 = ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type Vote2 = ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type VoteStorage2 = vote_storage::bitmap::Bitmap<
		ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>,
	>;

	// the actual state machine and consensus mechanisms of this ES
	type StateMachine = BWStateMachine<BitcoinDepositChannelWitnessingDefinition>;
	type ConsensusMechanism = BWConsensus<BlockData, btc::BlockNumber, ElectionProperties>;
}

/// Generating the state machine-based electoral system
pub type BitcoinDepositChannelWitnessing =
	StateMachineESInstance<BitcoinDepositChannelWitnessingDefinition>;

#[derive(
	Clone,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
	Default,
)]
pub struct BitcoinDepositChannelWitnessingGenerator;

impl Hook<btc::BlockNumber, Vec<DepositChannelDetails<Runtime, BitcoinInstance>>>
	for BitcoinDepositChannelWitnessingGenerator
{
	fn run(
		&mut self,
		block_witness_root: btc::BlockNumber,
	) -> Vec<DepositChannelDetails<Runtime, BitcoinInstance>> {
		// TODO: Channel expiry
		BitcoinIngressEgress::active_deposit_channels_at(block_witness_root)
	}
}

pub type BitcoinLiveness = Liveness<
	BlockNumber,
	Hash,
	cf_primitives::BlockNumber,
	ReportFailedLivenessCheck<Bitcoin>,
	<Runtime as Chainflip>::ValidatorId,
>;

pub struct BitcoinElectionHooks;

impl Hooks<BitcoinBlockHeightTracking, BitcoinDepositChannelWitnessing, BitcoinLiveness>
	for BitcoinElectionHooks
{
	fn on_finalize(
		(block_height_tracking_identifiers, deposit_channel_witnessing_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<BitcoinBlockHeightTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinDepositChannelWitnessing as ElectoralSystemTypes>::ElectionIdentifierExtra,
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
		let chain_progress = BitcoinBlockHeightTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinBlockHeightTracking,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(block_height_tracking_identifiers, &Vec::from([()]))?;

		log::info!("BitcoinElectionHooks::on_finalize: {:?}", chain_progress);
		BitcoinDepositChannelWitnessing::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinDepositChannelWitnessing,
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
			BWSettings { max_concurrent_elections: 15 },
			(),
		),
		settings: (Default::default(), Default::default(), LIVENESS_CHECK_DURATION),
	}
}
