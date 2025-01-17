use std::cmp::PartialEq;
use std::collections::BTreeMap;
use crate::{BitcoinChainTracking, BitcoinIngressEgress, Block, Runtime};
use cf_chains::{
	btc::{self, BitcoinFeeInfo, BitcoinTrackedData},
	Bitcoin,
};
use cf_traits::Chainflip;
use serde::{Deserialize, Serialize};
use sp_core::Get;

use cf_chains::instances::BitcoinInstance;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::__private::sp_tracing::event;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
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
use cf_chains::btc::BlockNumber;

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
	const BLOCK_BUFFER_SIZE: usize = 6;
	type ChainBlockNumber = btc::BlockNumber;
	type ChainBlockHash = btc::Hash;
	type BlockHeightChangeHook = BitcoinBlockHeightChangeHook;
}

/// Associating the ES related types to the struct
impl ESInterface for BitcoinBlockHeightTrackingTypes {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState = BHWStateWrapper<BitcoinBlockHeightTrackingTypes>;
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
	fn run(&self, block_height: btc::BlockNumber) {
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

impl Hook<(), SafeModeStatus> for BitcoinSafemodeEnabledHook {
	fn run(&self, _input: ()) -> SafeModeStatus {
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
}

/// Associating the ES related types to the struct
impl ESInterface for BitcoinDepositChannelWitnessingDefinition {
	type ValidatorId = <Runtime as Chainflip>::ValidatorId;
	type ElectoralUnsynchronisedState = BWState<BitcoinDepositChannelWitnessingDefinition>;
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
type Age = BlockNumber;
#[derive(PartialEq)]
enum BtcEvent {
	PreWitness(DepositWitness<Bitcoin>),
	Witness(DepositWitness<Bitcoin>),
}
pub struct BitcoinDepositChannelWitessingProcessor {
	pub blocks_data: BTreeMap<BlockNumber, (BlockData, Age)>,
	pub reorg_events: BTreeMap<BlockNumber, Vec<BtcEvent>>,
	pub rules: Vec<fn(Age, &BlockData) -> Vec<BtcEvent>>,
}


impl BitcoinDepositChannelWitessingProcessor {
	/// Insert a new BlockData, replacing the old one if another BlockData for the same height was already present
	/// NB! Replacement should never happen since when we detect a reorg we remove the block being re-orged
	fn insert(&mut self, n: BlockNumber, block_data: BlockData) {
		self.blocks_data.insert(n, (block_data, 0));
	}

	/// remove all the old BlockData and reorg_events based on the last block_height
	/// TODO! if we skip blocks we don't delete all the entries, handle this case
	fn clean_old(&mut self, n: BlockNumber) {
		self.blocks_data.remove(n - SAFETY_MARGIN);
		self.reorg_events.remove(n - BUFFER_EVENTS);
	}

	/// This function is responsible to process all the rules (in the correct order or with the correct logic) and return a list of Events
	/// I.E. if we end up with both a PreWitness and a Witness event for the same deposit we remove the PreWitness one
	/// TODO! implement this logic to remove PreWitness event in case a Witness is present
	fn process_rules(&self, last_height: BlockNumber) -> Vec<BtcEvent> {
		let mut last_events = vec![];
		for (block, (data, last_age)) in self.blocks_data {
			for age in last_age+1..=last_height-block {
				last_events = last_events.iter().chain(self.process_rules_for_age(age, &data)).collect();
			}
			*last_age = last_height - block;
		}
		last_events
	}

	/// This function is responsible to call all the rules on a given block and a given age of that block, it also compares the produced events against reorg_events and filter out duplicates
	fn process_rules_for_age(&self, age: Age, data: &BlockData) -> Vec<BtcEvent> {
		let mut events = vec![];
		for rule in self.rules {
			events = events.iter().chain(rule(age, data).iter()).collect();
		}
		events.into_iter().filter(|last_event| {
			for (_, events) in self.reorg_events {
				for event in events {
					if last_event == event {
						return false;
					}
				}
			}
			true
		}).collect::<Vec<_>>()
	}

	/// This function is responsible to execute all the previously produced events
	fn execute_events(events: Vec<BtcEvent>){
		for event in events {
			match event {
				BtcEvent::PreWitness(deposit) => {
					let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(
						deposit,
						deposit.block_number,
					);
				}
				BtcEvent::Witness(deposit) => {
					BitcoinIngressEgress::process_channel_deposit_full_witness(
						deposit,
						deposit_block_number,
					);
				}
			}
		}
	}
}

impl ProcessBlockData<btc::BlockNumber, Vec<DepositWitness<Bitcoin>>>
	for BitcoinDepositChannelWitessingProcessor
{
	fn process_block_data(
		&mut self,
		chain_progress: ChainProgress<btc::BlockNumber>,
	) {
		match chain_progress {
			ChainProgress::Progress(last_height)=> {
				let last_events = self.process_rules(last_height);
				self.execute(last_events);
				self.clean_old(last_height);
			},
			ChainProgress::Reorg(range) => {
				for n in range {
					let block_data = self.blocks_data.remove(n);
					if let Some((data, last_age)) = block_data {
						// We need to get only events already processed
						for age in 0..=last_age {
							self.reorg_events.insert(n, self.process_rules_for_age(age, &data));
						}
					}
				}
			},
			_ => {}
		}

		// ProcessedUpTo::<Runtime, BitcoinInstance>::put(
		// 	earliest_unprocessed_block.saturating_sub(1),
		// );
		//
		// // TODO: Handle reorgs, in particular when data is already processed.
		// // We need to ensure that we don't process the same data twice. We could use a wrapper for
		// // the BlockData type here that can include some extra status data in it.
		//
		// if witnesses.is_empty() {
		// 	log::info!("No witnesses to process for block: {:?}", current_block);
		// } else {
		// 	log::info!("Processing witnesses: {:?} for block {:?}", witnesses, current_block);
		// }
		// for (deposit_block_number, deposits) in witnesses.clone() {
		// 	for deposit in deposits {
		// 		if deposit_block_number == current_block {
		// 			log::info!("Prewitness deposit submitted by election: {:?}", deposit);
		// 			let _ = BitcoinIngressEgress::process_channel_deposit_prewitness(
		// 				deposit,
		// 				deposit_block_number,
		// 			);
		// 		} else if let Some(safety_margin) =
		// 			WitnessSafetyMargin::<Runtime, BitcoinInstance>::get()
		// 		{
		// 			if deposit_block_number <= (current_block - safety_margin) {
		// 				log::info!("deposit election submitted by election: {:?}", deposit);
		// 				BitcoinIngressEgress::process_channel_deposit_full_witness(
		// 					deposit,
		// 					deposit_block_number,
		// 				);
		// 			}
		// 		}
		// 	}
		// }
		//
		// // Do we need to return anything here?
		// witnesses
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
