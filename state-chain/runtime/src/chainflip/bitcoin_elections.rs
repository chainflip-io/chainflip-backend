use crate::{BitcoinIngressEgress, Runtime};
use cf_chains::{btc, Bitcoin};
use cf_traits::Chainflip;
use serde::{Deserialize, Serialize};
use sp_core::Get;

use cf_chains::instances::BitcoinInstance;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus, BHWState, BlockHeightTrackingProperties,
			BlockHeightTrackingSM, BlockHeightTrackingTypes, ChainProgress, InputHeaders,
		},
		block_witnesser::{
			consensus::BWConsensus,
			state_machine::{BWSettings, BWState, BWStateMachine, BWTypes},
			BlockElectionPropertiesGenerator, BlockWitnesser, ProcessBlockData,
		},
		composite::{
			tuple_2_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		state_machine::{
			core::{ConstantIndex, Hook, MultiIndexAndValue},
			state_machine_es::{ESInterface, StateMachineES, StateMachineESInstance},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};

use pallet_cf_ingress_egress::{
	DepositChannelDetails, DepositWitness, PalletSafeMode, ProcessedUpTo, WitnessSafetyMargin,
};
use scale_info::TypeInfo;

use sp_std::vec::Vec;

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(BitcoinBlockHeightTracking, BitcoinDepositChannelWitnessing),
	<Runtime as Chainflip>::ValidatorId,
	RunnerStorageAccess<Runtime, BitcoinInstance>,
	BitcoinElectionHooks,
>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OpenChannelDetails<ChainBlockNumber> {
	pub open_block: ChainBlockNumber,
	pub close_block: ChainBlockNumber,
}

pub type OldBitcoinDepositChannelWitnessing = BlockWitnesser<
	Bitcoin,
	Vec<DepositWitness<Bitcoin>>,
	Vec<DepositChannelDetails<Runtime, BitcoinInstance>>,
	<Runtime as Chainflip>::ValidatorId,
	BitcoinDepositChannelWitessingProcessor,
	BitcoinDepositChannelWitnessingGenerator,
>;

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
	const SAFETY_MARGIN: usize = 6;
	type ChainBlockNumber = btc::BlockNumber;
	type ChainBlockHash = btc::Hash;
}

/// Associating the ES related types to the struct
impl ESInterface for BitcoinBlockHeightTrackingTypes {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState = BHWState<BitcoinBlockHeightTrackingTypes>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = BlockHeightTrackingProperties<btc::BlockNumber>;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<InputHeaders<BitcoinBlockHeightTrackingTypes>>;
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
type BlockData = Vec<DepositWitness<Bitcoin>>;

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

impl Hook<(), bool> for BitcoinSafemodeEnabledHook {
	fn run(&self, _input: ()) -> bool {
		!<<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
			PalletSafeMode<BitcoinInstance>,
		>>::get()
		.deposits_enabled
	}
}

/// Associating BW types to the struct
impl BWTypes<btc::BlockNumber> for BitcoinDepositChannelWitnessingDefinition {
	type ElectionProperties = ElectionProperties;
	type ElectionPropertiesHook = BitcoinDepositChannelWitnessingGenerator;
	type SafeModeEnabledHook = BitcoinSafemodeEnabledHook;
}

/// Associating the ES related types to the struct
impl ESInterface for BitcoinDepositChannelWitnessingDefinition {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState =
		BWState<btc::BlockNumber, BitcoinDepositChannelWitnessingDefinition>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = BWSettings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (btc::BlockNumber, ElectionProperties, u8);
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<
		ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>,
	>;
	type Consensus = MultiIndexAndValue<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type OnFinalizeContext = Vec<ChainProgress<btc::BlockNumber>>;
	type OnFinalizeReturn = Vec<()>;
}

/// Associating the state machine and consensus mechanism to the struct
impl StateMachineES for BitcoinDepositChannelWitnessingDefinition {
	// both context and return have to be vectors, these are the item types
	type OnFinalizeContextItem = ChainProgress<btc::BlockNumber>;
	type OnFinalizeReturnItem = ();

	// restating types since we have to prove that they have the correct bounds
	type Consensus2 = MultiIndexAndValue<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type Vote2 = ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>;
	type VoteStorage2 = vote_storage::bitmap::Bitmap<
		ConstantIndex<(btc::BlockNumber, ElectionProperties, u8), BlockData>,
	>;

	// the actual state machine and consensus mechanisms of this ES
	type StateMachine =
		BWStateMachine<BitcoinDepositChannelWitnessingDefinition, BlockData, btc::BlockNumber>;
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

impl
	BlockElectionPropertiesGenerator<
		btc::BlockNumber,
		Vec<DepositChannelDetails<Runtime, BitcoinInstance>>,
	> for BitcoinDepositChannelWitnessingGenerator
{
	fn generate_election_properties(
		block_witness_root: btc::BlockNumber,
	) -> Vec<DepositChannelDetails<Runtime, BitcoinInstance>> {
		// TODO: Channel expiry
		BitcoinIngressEgress::active_deposit_channels_at(block_witness_root)
	}
}

impl Hook<btc::BlockNumber, Vec<DepositChannelDetails<Runtime, BitcoinInstance>>>
	for BitcoinDepositChannelWitnessingGenerator
{
	fn run(
		&self,
		block_witness_root: btc::BlockNumber,
	) -> Vec<DepositChannelDetails<Runtime, BitcoinInstance>> {
		// TODO: Channel expiry
		BitcoinIngressEgress::active_deposit_channels_at(block_witness_root)
	}
}

pub struct BitcoinDepositChannelWitessingProcessor;

impl ProcessBlockData<btc::BlockNumber, Vec<DepositWitness<Bitcoin>>>
	for BitcoinDepositChannelWitessingProcessor
{
	fn process_block_data(
		current_block: btc::BlockNumber,
		earliest_unprocessed_block: btc::BlockNumber,
		witnesses: Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)>,
	) -> Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)> {
		ProcessedUpTo::<Runtime, BitcoinInstance>::put(
			earliest_unprocessed_block.saturating_sub(1),
		);

		// TODO: Handle reorgs, in particular when data is already processed.
		// We need to ensure that we don't process the same data twice. We could use a wrapper for
		// the BlockData type here that can include some extra status data in it.

		if witnesses.is_empty() {
			log::info!("No witnesses to process for block: {:?}", current_block);
		} else {
			log::info!("Processing witnesses: {:?} for block {:?}", witnesses, current_block);
		}
		for (deposit_block_number, deposits) in witnesses.clone() {
			for deposit in deposits {
				if deposit_block_number == current_block {
					log::info!("Prewitness deposit submitted by election: {:?}", deposit);
					let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(
						deposit,
						deposit_block_number,
					);
				} else if let Some(safety_margin) =
					WitnessSafetyMargin::<Runtime, BitcoinInstance>::get()
				{
					if deposit_block_number <= (current_block - safety_margin) {
						log::info!("deposit election submitted by election: {:?}", deposit);
						BitcoinIngressEgress::process_channel_deposit_full_witness(
							deposit,
							deposit_block_number,
						);
					}
				}
			}
		}

		// Do we need to return anything here?
		witnesses
	}
}

pub struct BitcoinElectionHooks;

impl Hooks<BitcoinBlockHeightTracking, BitcoinDepositChannelWitnessing> for BitcoinElectionHooks {
	fn on_finalize(
		(block_height_tracking_identifiers, deposit_channel_witnessing_identifiers): (
			Vec<
				ElectionIdentifier<
					<BitcoinBlockHeightTracking as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<BitcoinDepositChannelWitnessing as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
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

		Ok(())
	}
}

// Channel expiry:
// We need to process elections in order, even after a safe mode pause. This is to ensure channel
// expiry is done correctly. During safe mode pause, we could get into a situation where the current
// state suggests that a channel is expired, but at the time of a previous block which we have not
// yet processed, the channel was not expired.

pub fn initial_state() -> InitialStateOf<Runtime, BitcoinInstance> {
	InitialState {
		unsynchronised_state: (Default::default(), Default::default()),
		unsynchronised_settings: (
			Default::default(),
			// TODO: Write a migration to set this too.
			BWSettings { max_concurrent_elections: 5 },
		),
		settings: (Default::default(), Default::default()),
	}
}
