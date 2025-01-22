use crate::{BitcoinChainTracking, BitcoinIngressEgress, Block, ConstU32, Runtime};
use cf_chains::{
	btc::{self, BitcoinFeeInfo, BitcoinTrackedData},
	instances::BitcoinInstance,
	Bitcoin,
};
use cf_traits::Chainflip;
use core::ops::RangeInclusive;
use serde::{Deserialize, Serialize};
use sp_core::Get;
use sp_std::collections::btree_map::BTreeMap;

use frame_support::__private::sp_tracing::event;
use log::warn;
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
            state_machine::{
				BWSettings, BWState, BWStateMachine, BWTypes, BlockWitnesserProcessor,
			},
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
use sp_core::{Decode, Encode, MaxEncodedLen};

use pallet_cf_ingress_egress::{
	DepositChannelDetails, DepositWitness, PalletSafeMode, ProcessedUpTo, WitnessSafetyMargin,
};
use scale_info::TypeInfo;
use sp_runtime::BoundedVec;

use cf_chains::btc::BlockNumber;
use pallet_cf_elections::electoral_systems::block_witnesser::primitives::ChainProgressInner;
use sp_std::{vec, vec::Vec};

const SAFETY_MARGIN: BlockNumber = 10;
const BUFFER_EVENTS: BlockNumber = 10;

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
	type Event = BtcEvent;
	type BlockProcessor =
		DepositChannelWitessingProcessor<Self::ChainBlockNumber, Self::BlockData, Self::Event>;
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub enum BtcEvent {
	PreWitness(BlockNumber, DepositWitness<Bitcoin>),
	Witness(BlockNumber, DepositWitness<Bitcoin>),
}
impl BtcEvent {
	pub fn deposit_witness(&self) -> &DepositWitness<Bitcoin> {
		match self {
			BtcEvent::PreWitness(_, dw) | BtcEvent::Witness(_, dw) => dw,
		}
	}
	pub fn equal_inner(&self, other: &BtcEvent) -> bool {
		self.deposit_witness() == other.deposit_witness()
	}
}

/// Returns one event per deposit witness. If multiple events share the same deposit witness:
/// - prefer the `Witness` variant (override any `PreWitness`),
/// - otherwise keep the single `PreWitness`.
pub fn deduplicate_btc_events(events: Vec<BtcEvent>) -> Vec<BtcEvent> {
	// Map: deposit_witness -> chosen BtcEvent
	let mut chosen: BTreeMap<DepositWitness<Bitcoin>, BtcEvent> = BTreeMap::new();

	for event in events {
		let deposit: DepositWitness<Bitcoin> = event.deposit_witness().clone();

		match chosen.get(&deposit) {
			None => {
				// No event yet for this deposit, store it
				chosen.insert(deposit, event);
			},
			Some(existing) => {
				// There's already an event for this deposit
				match (existing, &event) {
					// If we already have a Witness, do nothing
					(BtcEvent::Witness(_, _), BtcEvent::PreWitness(_, _)) => (),
					// If we have a PreWitness and the new event is a Witness, override it
					(BtcEvent::PreWitness(_, _), BtcEvent::Witness(_, _)) => {
						chosen.insert(deposit, event);
					},
					// This should be impossible to reach!
					(_, _) => (),
				}
			},
		}
	}

	chosen.into_values().collect()
}

#[derive(
	Debug,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Deserialize,
	Serialize,
	MaxEncodedLen,
	Default,
)]
enum BtcRuleType {
	PreWitness,
	Witness,
	#[default]
	None,
}
fn apply_rule(
	block: BlockNumber,
	rule: &BtcRuleType,
	age: Age,
	block_data: &BlockData,
) -> Vec<BtcEvent> {
	match rule {
		BtcRuleType::PreWitness =>
			if age == 0 {
				block_data
					.iter()
					.map(|deposit_witness| BtcEvent::PreWitness(block, deposit_witness.clone()))
					.collect()
			} else {
				vec![]
			},
		BtcRuleType::Witness =>
			if age == SAFETY_MARGIN - 1 {
				block_data
					.iter()
					.map(|deposit_witness| BtcEvent::Witness(block, deposit_witness.clone()))
					.collect()
			} else {
				vec![]
			},
		_ => {
			vec![]
		},
	}
}

#[derive(
	Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
pub struct DepositChannelWitessingProcessor<ChainBlockNumber: Ord, BlockData, Event> {
	pub blocks_data: BTreeMap<ChainBlockNumber, (BlockData, Age)>,
	pub reorg_events: BTreeMap<ChainBlockNumber, Vec<Event>>,
	pub rules: BoundedVec<BtcRuleType, ConstU32<10>>,
}
impl<ChainBlockNumber: Ord, BlockData, Event> Default
	for DepositChannelWitessingProcessor<ChainBlockNumber, BlockData, Event>
{
	fn default() -> Self {
		Self {
			blocks_data: Default::default(),
			reorg_events: Default::default(),
			rules: {
				let mut result = BoundedVec::<BtcRuleType, ConstU32<10>>::new();
				result.try_push(BtcRuleType::PreWitness).expect("cannot fail");
				result.try_push(BtcRuleType::Witness).expect("cannot fail");
				result
			},
		}
	}
}

impl BlockWitnesserProcessor<BlockNumber, BlockData, BtcEvent>
	for DepositChannelWitessingProcessor<BlockNumber, BlockData, BtcEvent>
{
	fn process_block_data(&mut self, chain_progress: ChainProgressInner<BlockNumber>) {
		match chain_progress {
			ChainProgressInner::Progress(last_height) => {
				let last_events = deduplicate_btc_events(self.process_rules(last_height));
				self.execute_events(last_events);
				self.clean_old(last_height);
			},
			ChainProgressInner::Reorg(range) => {
				for n in range {
					let block_data = self.blocks_data.remove(&n);
					if let Some((data, last_age)) = block_data {
						// We need to get only events already processed
						for age in 0..=last_age {
							self.reorg_events
								.insert(n, self.process_rules_for_age_and_block(n, age, &data));
						}
					}
				}
			},
		}
	}

	fn insert(&mut self, n: BlockNumber, block_data: BlockData) {
		// warn!("Inserting new blockdata for block: {n:#?}: {block_data:#?}");
		self.blocks_data.insert(n, (block_data, 0));
	}

	fn clean_old(&mut self, n: BlockNumber) {
		self.blocks_data.retain(|key, (data, age)| *age <= SAFETY_MARGIN);
		self.reorg_events.remove(&(n - BUFFER_EVENTS));
	}

	fn process_rules(&mut self, last_height: BlockNumber) -> Vec<BtcEvent> {
		warn!("Processing rules... last_height: {last_height:#?}");
		let mut last_events: Vec<BtcEvent> = vec![];
		for (block, (data, last_age)) in self.blocks_data.clone() {
			warn!("Rules for block {block:?}, age: {last_age:?}, data: {data:?}");
			for age in last_age..=last_height - block {
				last_events = last_events
					.into_iter()
					.chain(self.process_rules_for_age_and_block(block, age, &data))
					.collect();
			}
			self.blocks_data.insert(block, (data.clone(), last_height - block + 1));
		}
		warn!("Rules produced these events: {last_events:#?}");

		last_events
	}

	fn process_rules_for_age_and_block(
		&self,
		block: BlockNumber,
		age: BlockNumber,
		data: &BlockData,
	) -> Vec<BtcEvent> {
		let mut events: Vec<BtcEvent> = vec![];
		for rule in &self.rules {
			events = events.into_iter().chain(apply_rule(block, rule, age, data)).collect();
		}
		events
			.into_iter()
			.filter(|last_event| {
				for (_, events) in &self.reorg_events {
					for event in events {
						if last_event.equal_inner(event) {
							return false;
						}
					}
				}
				true
			})
			.collect::<Vec<_>>()
	}

	fn execute_events(&self, events: Vec<BtcEvent>) {
		warn!("Executing these events: {events:#?}");
		for event in events {
			match event {
				BtcEvent::PreWitness(block, deposit) => {
					let _ =
						BitcoinIngressEgress::process_channel_deposit_prewitness(deposit, block);
				},
				BtcEvent::Witness(block, deposit) => {
					BitcoinIngressEgress::process_channel_deposit_full_witness(deposit, block);
					warn!("Witness executed");
				},
			}
		}
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
			BWSettings { max_concurrent_elections: 15 },
		),
		settings: (Default::default(), Default::default()),
	}
}
