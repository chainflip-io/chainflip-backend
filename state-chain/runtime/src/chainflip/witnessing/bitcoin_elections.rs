use crate::{
	chainflip::{
		address_derivation::btc::derive_current_and_previous_epoch_private_btc_vaults,
		witnessing::pallet_hooks, ReportFailedLivenessCheck,
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
	witness_period::SaturatingStep,
	Bitcoin, Chain, DepositChannel,
};
use cf_primitives::{AccountId, ChannelId};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{Chainflip, Hook};
use cf_utilities::{cargo_fmt_ignore, derive_common_traits, impls};
use codec::DecodeWithMemTracking;
use core::ops::RangeInclusive;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_broadcast::{TransactionConfirmation, TransactionOutIdToBroadcastId};
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_system_runner::RunnerStorageAccessTrait,
	electoral_systems::{
		block_height_witnesser::{
			consensus::BlockHeightWitnesserConsensus, primitives::NonemptyContinuousHeaders,
			state_machine::BlockHeightWitnesser, BHWTypes, BlockHeightChangeHook,
			BlockHeightWitnesserSettings, ChainBlockNumberOf, ChainProgress, ChainTypes, ReorgHook,
		},
		block_witnesser::{
			instance::{
				BlockWitnesserInstance, GenericBlockWitnesser, JustWitnessAtSafetyMargin,
				PrewitnessImmediatelyAndWitnessAtSafetyMargin,
			},
			state_machine::{BWElectionType, BWTypes, BlockWitnesserSettings, HookTypeFor},
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
	vote_storage, CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use pallet_cf_ingress_egress::{DepositWitness, ProcessedUpTo, VaultDepositWitness};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode, Get};
use sp_runtime::RuntimeDebug;
use sp_std::vec::Vec;

use super::elections::TypesFor;

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(
		BitcoinBlockHeightWitnesserES,
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

pub struct BitcoinChainTag;
pub type BitcoinChain = TypesFor<BitcoinChainTag>;
impl ChainTypes for BitcoinChain {
	type ChainBlockNumber = btc::BlockNumber;
	type ChainBlockHash = btc::Hash;
	const NAME: &'static str = "Bitcoin";
}

#[derive(Clone, Eq, PartialEq, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo)]
pub enum BitcoinElectoralEvents {
	ReorgDetected { reorged_blocks: RangeInclusive<btc::BlockNumber> },
}
// ------------------------ block height tracking ---------------------------
/// The electoral system for block height tracking
pub struct BitcoinBlockHeightWitnesser;

impls! {
	for TypesFor<BitcoinBlockHeightWitnesser>:

	/// Associating the SM related types to the struct
	BHWTypes {
		type BlockHeightChangeHook = Self;
		type Chain = BitcoinChain;
		type ReorgHook = Self;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StatemachineElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type VoteStorage = vote_storage::bitmap::Bitmap<NonemptyContinuousHeaders<BitcoinChain>>;

		type OnFinalizeReturnItem = Option<ChainProgress<BitcoinChain>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightWitnesserConsensus<Self>;
		type Statemachine = BlockHeightWitnesser<Self>;
	}

	Hook<HookTypeFor<Self, BlockHeightChangeHook>> {
		fn run(&mut self, block_height: btc::BlockNumber) {
			if let Err(err) = BitcoinChainTracking::inner_update_chain_height(block_height) {
				log::error!("Failed to update BTC chain height to {block_height:?}: {:?}", err);
			}
		}
	}

	Hook<HookTypeFor<Self, ReorgHook>> {
		fn run(&mut self, reorged_blocks: RangeInclusive<btc::BlockNumber>) {
			pallet_cf_elections::Pallet::<Runtime, BitcoinInstance>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(BitcoinElectoralEvents::ReorgDetected {
					reorged_blocks
				})
			);
		}
	}
}

/// Generating the state machine-based electoral system
pub type BitcoinBlockHeightWitnesserES =
	StatemachineElectoralSystem<TypesFor<BitcoinBlockHeightWitnesser>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct BitcoinDepositChannelWitnessing;
impl BlockWitnesserInstance for TypesFor<BitcoinDepositChannelWitnessing> {
	const BWNAME: &'static str = "DepositChannel";
	type Runtime = Runtime;
	type Chain = BitcoinChain;
	type BlockEntry = DepositWitness<Bitcoin>;
	type ElectionProperties = Vec<DepositChannel<Bitcoin>>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BitcoinInstance>;
	type WitnessRules = PrewitnessImmediatelyAndWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
		>>::get()
		.deposit_channel_witnessing_enabled
	}

	fn election_properties(height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties {
		BitcoinIngressEgress::active_deposit_channels_at(
			// we advance by SAFETY_BUFFER before checking opened_at
			height.saturating_forward(BITCOIN_MAINNET_SAFETY_BUFFER as usize),
			// we don't advance for expiry
			height,
		)
		.into_iter()
		.map(|deposit_channel_details| deposit_channel_details.deposit_channel)
		.collect()
	}

	fn processed_up_to(up_to: ChainBlockNumberOf<Self::Chain>) {
		// we go back SAFETY_BUFFER, such that we only actually expire once this amount of blocks
		// have been additionally processed.
		ProcessedUpTo::<Runtime, BitcoinInstance>::set(
			up_to.saturating_backward(BITCOIN_MAINNET_SAFETY_BUFFER as usize),
		);
	}

	cargo_fmt_ignore! {
	// --------------------- Interaction with deposit channels --------------------- //

	// We apply the SAFETY_BUFFER before we fetch the election_properties,
	// this makes sure that if txs that have been submitted post channel creation,
	// but due to reorg ended up in an external chain block that's below the channels `opened_at`,
	// are still witnessed. (See PRO-2306).
	//
	// We also apply SAFETY_BUFFER before expiring a deposit channel, in case there are reorgs
	// which reorder transactions such they move back by a few blocks and are now within the valid
	// range of a deposit channel.
	//
	// Thus, we have the following setup:
	//
	//                                   /- All deposits that happen in the SAFETY_BUFFER and are reorged
	//                                   |  to *before* the expiry of the previous channel are going to be
	//                                   |  witnessed for it.
	//                                   |
	//                                   |  All deposits that get into blocks after expire_at, inside the SAFETY_BUFFER,
	//                                   |  are not going to be witnessed for any channel.
	//                                   |
	//                                   |                 /- Deposits that are made into the new channel could be reorged
	//                                   |                 |  to blocks that are before opened_at. We apply the SAFETY_BUFFER
	//                                   |                 |  and witness txs for a deposit channel even if they occur in blocks
	//                                   |                 |  before opened_at.
	//                                   |                 |
	//                                   |  |<------------------------------------...->
	//                                   v  |<-- SAFETY -->|
	// |---- previous channel ----|--------------|---------|---- new channel -----...->
	//                            |<-- SAFETY -->|         ^ opened_at
	//                            ^ expire_at
	//
	// Critical case: we want to ensure that no deposits are double-witnessed. Let's say a boosted deposit for the previous
	// channel is witnessed before expire_at of that channel. The deposit is ingressed. We now have a reorg which moves the
	// deposit behind expire_at. In the meantime, the chain progresses SAFETY_BUFFER blocks, the channel is recycled and reused.
	// Witnessing for the new channel begins SAFETY_BUFFER before opened_at, and thus includes the previously (reorged) deposit.
	// Now let's say we have another reorg, which makes us rewitness the block with the deposit. This election is going to have
	// the new deposit channel in its election properties and thus will witness the tx again. We won't emit a PreWitness event
	// though, since the tx was already prewitnessed and reorged, and the blockprocessor filters out the already emitted events.
	// **BUT**: If there wasn't emitted a Witness event previously for this tx, then we will now emit a Witness event into the new
	// channel!
	//
	// CONCLUSION: There has to be at least 2*SAFETY_BUFFER distance between `expire_at` of the previous channel and `opened_at` of the recycled
	// channel.
	//
	}
}

/// Generating the state machine-based electoral system
pub type BitcoinDepositChannelWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TypesFor<BitcoinDepositChannelWitnessing>>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing
pub struct BitcoinVaultDepositWitnessing;

impl BlockWitnesserInstance for TypesFor<BitcoinVaultDepositWitnessing> {
	const BWNAME: &'static str = "VaultDeposit";
	type Runtime = Runtime;
	type Chain = BitcoinChain;
	type BlockEntry = VaultDepositWitness<Runtime, BitcoinInstance>;
	type ElectionProperties = Vec<(DepositAddress, AccountId, ChannelId)>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BitcoinInstance>;
	type WitnessRules = PrewitnessImmediatelyAndWitnessAtSafetyMargin<Self::BlockEntry>;

	fn election_properties(_height: ChainBlockNumberOf<Self::Chain>) -> Self::ElectionProperties {
		// WARNING: If a private broker channel is closed by the broker while safe mode is
		// activated, we will miss vault deposits to that channel that happened during safe mode.
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

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_ingress_egress::Config<BitcoinInstance>>::SafeMode as Get<
			pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
		>>::get()
		.vault_deposit_witnessing_enabled
	}

	fn processed_up_to(_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

/// Generating the state machine-based electoral system
pub type BitcoinVaultDepositWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TypesFor<BitcoinVaultDepositWitnessing>>>;

// ------------------------ egress witnessing ---------------------------
/// The electoral system for egress witnessing
pub struct BitcoinEgressWitnessing;

impl BlockWitnesserInstance for TypesFor<BitcoinEgressWitnessing> {
	const BWNAME: &'static str = "Egress";
	type Runtime = Runtime;
	type Chain = BitcoinChain;
	type BlockEntry = TransactionConfirmation<Runtime, BitcoinInstance>;
	type ElectionProperties = Vec<Hash>;
	type ExecutionTarget = pallet_hooks::PalletHooks<Runtime, BitcoinInstance>;
	type WitnessRules = JustWitnessAtSafetyMargin<Self::BlockEntry>;

	fn is_enabled() -> bool {
		<<Runtime as pallet_cf_broadcast::Config<BitcoinInstance>>::SafeMode as Get<
			pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
		>>::get()
		.egress_witnessing_enabled
	}

	fn election_properties(
		_block_height: ChainBlockNumberOf<Self::Chain>,
	) -> Self::ElectionProperties {
		TransactionOutIdToBroadcastId::<Runtime, BitcoinInstance>::iter()
			.map(|(tx_id, _)| tx_id)
			.collect::<Vec<_>>()
	}

	fn processed_up_to(_block_height: ChainBlockNumberOf<Self::Chain>) {
		// NO-OP (processed_up_to is required only for deposit channels)
	}
}

/// Generating the state machine-based electoral system
pub type BitcoinEgressWitnessingES =
	StatemachineElectoralSystem<GenericBlockWitnesser<TypesFor<BitcoinEgressWitnessing>>>;

// ------------------------ liveness ---------------------------
pub type BitcoinLiveness = Liveness<
	BlockNumber,
	Hash,
	ReportFailedLivenessCheck<Bitcoin>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ fee tracking ---------------------------
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
derive_common_traits! {
	#[derive(TypeInfo)]
	pub struct BitcoinFeeSettings {
		pub tx_sample_count_per_mempool_block: u32,
		pub fixed_median_fee_adjustement_sat_per_vkilobyte: BtcAmount,
	}
}
pub type BitcoinFeeTracking = UnsafeMedian<
	<Bitcoin as Chain>::ChainAmount,
	BitcoinFeeSettings,
	BitcoinFeeUpdateHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

// ------------------------ composite electoral system ---------------------------
pub struct BitcoinElectionHooks;

impl
	Hooks<
		BitcoinBlockHeightWitnesserES,
		BitcoinDepositChannelWitnessingES,
		BitcoinVaultDepositWitnessingES,
		BitcoinEgressWitnessingES,
		BitcoinFeeTracking,
		BitcoinLiveness,
	> for BitcoinElectionHooks
{
	fn on_finalize(
		(block_height_witnesser_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, egress_identifiers, fee_identifiers, liveness_identifiers): (
			Vec<
				ElectionIdentifier<
					<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionIdentifierExtra,
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

		let chain_progress = BitcoinBlockHeightWitnesserES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinBlockHeightWitnesserES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(block_height_witnesser_identifiers, &Vec::from([()]))?;

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
		>(fee_identifiers, &current_sc_block_number)?;

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
					.saturating_sub(BITCOIN_MAINNET_SAFETY_BUFFER.into()),
				crate::Validator::current_epoch(),
			),
		)?;

		Ok(())
	}
}

pub const BITCOIN_MAINNET_SAFETY_BUFFER: u32 = 8;

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
			BlockHeightWitnesserSettings { safety_buffer: BITCOIN_MAINNET_SAFETY_BUFFER },
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: BITCOIN_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 1,
				safety_buffer: BITCOIN_MAINNET_SAFETY_BUFFER,
			},
			BlockWitnesserSettings {
				max_ongoing_elections: 15,
				max_optimistic_elections: 1,
				safety_margin: 0,
				safety_buffer: BITCOIN_MAINNET_SAFETY_BUFFER,
			},
			10, // wait 10 SC blocks until reopening fee election
			(),
		),
		settings: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			BitcoinFeeSettings {
				tx_sample_count_per_mempool_block: 20,
				fixed_median_fee_adjustement_sat_per_vkilobyte: 1000,
			},
			LIVENESS_CHECK_DURATION,
		),
		shared_data_reference_lifetime: 8,
	}
}

pub struct BitcoinElectoralSystemConfiguration;

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum ElectionTypes {
	DepositChannels(<GenericBlockWitnesser<TypesFor<BitcoinDepositChannelWitnessing>> as BWTypes>::ElectionProperties),
	Vaults(<GenericBlockWitnesser<TypesFor<BitcoinVaultDepositWitnessing>> as BWTypes>::ElectionProperties),
	Egresses(<GenericBlockWitnesser<TypesFor<BitcoinEgressWitnessing>> as BWTypes>::ElectionProperties),
}

impl pallet_cf_elections::ElectoralSystemConfiguration for BitcoinElectoralSystemConfiguration {
	type SafeMode = ();

	type ElectoralEvents = BitcoinElectoralEvents;

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
