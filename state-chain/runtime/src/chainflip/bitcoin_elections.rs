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
		Hash,
	},
	instances::BitcoinInstance,
	Bitcoin,
};
use cf_primitives::{AccountId, ChannelId};
use cf_runtime_utilities::log_or_panic;
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		block_height_tracking::{
			consensus::BlockHeightTrackingConsensus,
			state_machine::{BHWStateWrapper, BlockHeightTrackingSM, InputHeaders},
			BlockHeightChangeHook, BlockHeightTrackingProperties, BlockHeightTrackingTypes,
			ChainProgress,
		},
		block_witnesser::{
			consensus::BWConsensus,
			primitives::SafeModeStatus,
			state_machine::{
				BWProcessorTypes, BWSettings, BWState, BWStateMachine, BWTypes,
				ElectionPropertiesHook, HookTypeFor, SafeModeEnabledHook,
			},
		},
		composite::{
			tuple_4_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		liveness::Liveness,
		state_machine::{
			core::{ConstantIndex, Hook},
			state_machine_es::{StateMachineES, StateMachineESInstance},
		},
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
		BitcoinLiveness,
	),
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
	BlockHeightTrackingTypes {
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
		type ElectionProperties = BlockHeightTrackingProperties<btc::BlockNumber>;
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<InputHeaders<Self>>;
		type Consensus = InputHeaders<Self>;
		type OnFinalizeContext = Vec<()>;
		type OnFinalizeReturn = Vec<ChainProgress<btc::BlockNumber>>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StateMachineES {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ();
		type OnFinalizeReturnItem = ChainProgress<btc::BlockNumber>;

		// restating types since we have to prove that they have the correct bounds
		type Consensus2 = InputHeaders<Self>;
		type Vote2 = InputHeaders<Self>;
		type VoteStorage2 = vote_storage::bitmap::Bitmap<InputHeaders<Self>>;

		// the actual state machine and consensus mechanisms of this ES
		type ConsensusMechanism = BlockHeightTrackingConsensus<Self>;
		type StateMachine = BlockHeightTrackingSM<Self>;
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
	StateMachineESInstance<TypesFor<BitcoinBlockHeightTracking>>;

// ------------------------ deposit channel witnessing ---------------------------
/// The electoral system for deposit channel witnessing
pub struct BitcoinDepositChannelWitnessing;

type ElectionPropertiesDepositChannel = Vec<DepositChannelDetails<Runtime, BitcoinInstance>>;
pub(crate) type BlockDataDepositChannel = Vec<DepositWitness<Bitcoin>>;

impls! {
	for TypesFor<BitcoinDepositChannelWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type ChainBlockNumber = btc::BlockNumber;
		type BlockData = BlockDataDepositChannel;

		type Event = BtcEvent<DepositWitness<Bitcoin>>;
		type Rules = Self;
		type Execute = Self;
		type DedupEvents = Self;
		type SafetyMargin = Self;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesDepositChannel;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BWState<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = BWSettings;
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = (btc::BlockNumber, ElectionPropertiesDepositChannel, u8);
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<
			ConstantIndex<(btc::BlockNumber, ElectionPropertiesDepositChannel, u8), BlockDataDepositChannel>,
		>;
		type Consensus = ConstantIndex<(btc::BlockNumber, ElectionPropertiesDepositChannel, u8), BlockDataDepositChannel>;
		type OnFinalizeContext = Vec<ChainProgress<btc::BlockNumber>>;
		type OnFinalizeReturn = Vec<()>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StateMachineES {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgress<btc::BlockNumber>;
		type OnFinalizeReturnItem = ();

		// restating types since we have to prove that they have the correct bounds
		type Consensus2 = ConstantIndex<(btc::BlockNumber, ElectionPropertiesDepositChannel, u8), BlockDataDepositChannel>;
		type Vote2 = ConstantIndex<(btc::BlockNumber, ElectionPropertiesDepositChannel, u8), BlockDataDepositChannel>;
		type VoteStorage2 = vote_storage::bitmap::Bitmap<
			ConstantIndex<(btc::BlockNumber, ElectionPropertiesDepositChannel, u8), BlockDataDepositChannel>,
		>;

		// the actual state machine and consensus mechanisms of this ES
		type StateMachine = BWStateMachine<Self>;
		type ConsensusMechanism = BWConsensus<BlockDataDepositChannel, btc::BlockNumber, ElectionPropertiesDepositChannel>;
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
	StateMachineESInstance<TypesFor<BitcoinDepositChannelWitnessing>>;

// ------------------------ vault deposit witnessing ---------------------------
/// The electoral system for vault deposit witnessing

pub struct BitcoinVaultDepositWitnessing;

type ElectionPropertiesVaultDeposit = Vec<(DepositAddress, AccountId, ChannelId)>;
pub(crate) type BlockDataVaultDeposit = Vec<VaultDepositWitness<Runtime, BitcoinInstance>>;

impls! {
	for TypesFor<BitcoinVaultDepositWitnessing>:

	/// Associating BW processor types
	BWProcessorTypes {
		type ChainBlockNumber = BlockNumber;
		type BlockData = BlockDataVaultDeposit;

		type Event = BtcEvent<VaultDepositWitness<Runtime, BitcoinInstance>>;
		type Rules = Self;
		type Execute = Self;
		type DedupEvents = Self;
		type SafetyMargin = Self;
	}

	/// Associating BW types to the struct
	BWTypes {
		type ElectionProperties = ElectionPropertiesVaultDeposit;
		type ElectionPropertiesHook = Self;
		type SafeModeEnabledHook = Self;
	}

	/// Associating the ES related types to the struct
	ElectoralSystemTypes {
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type ElectoralUnsynchronisedState = BWState<Self>;
		type ElectoralUnsynchronisedStateMapKey = ();
		type ElectoralUnsynchronisedStateMapValue = ();
		type ElectoralUnsynchronisedSettings = BWSettings;
		type ElectoralSettings = ();
		type ElectionIdentifierExtra = ();
		type ElectionProperties = (btc::BlockNumber, ElectionPropertiesVaultDeposit, u8);
		type ElectionState = ();
		type VoteStorage = vote_storage::bitmap::Bitmap<
			ConstantIndex<(btc::BlockNumber, ElectionPropertiesVaultDeposit, u8), BlockDataVaultDeposit>,
		>;
		type Consensus = ConstantIndex<(btc::BlockNumber, ElectionPropertiesVaultDeposit, u8), BlockDataVaultDeposit>;
		type OnFinalizeContext = Vec<ChainProgress<btc::BlockNumber>>;
		type OnFinalizeReturn = Vec<()>;
	}

	/// Associating the state machine and consensus mechanism to the struct
	StateMachineES {
		// both context and return have to be vectors, these are the item types
		type OnFinalizeContextItem = ChainProgress<btc::BlockNumber>;
		type OnFinalizeReturnItem = ();

		// restating types since we have to prove that they have the correct bounds
		type Consensus2 = ConstantIndex<(btc::BlockNumber, ElectionPropertiesVaultDeposit, u8), BlockDataVaultDeposit>;
		type Vote2 = ConstantIndex<(btc::BlockNumber, ElectionPropertiesVaultDeposit, u8), BlockDataVaultDeposit>;
		type VoteStorage2 = vote_storage::bitmap::Bitmap<
			ConstantIndex<(btc::BlockNumber, ElectionPropertiesVaultDeposit, u8), BlockDataVaultDeposit>,
		>;

		// the actual state machine and consensus mechanisms of this ES
		type StateMachine = BWStateMachine<Self>;
		type ConsensusMechanism = BWConsensus<BlockDataVaultDeposit, btc::BlockNumber, ElectionPropertiesVaultDeposit>;
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
						.flat_map(|addresses| addresses)
						.map(move |address| (address, broker_id.clone(), channel_id))
				})
				.collect::<Vec<_>>()
		}
	}

}

/// Generating the state machine-based electoral system
pub type BitcoinVaultDepositWitnessingES =
	StateMachineESInstance<TypesFor<BitcoinVaultDepositWitnessing>>;
pub type BitcoinLiveness = Liveness<
	BlockNumber,
	Hash,
	cf_primitives::BlockNumber,
	ReportFailedLivenessCheck<Bitcoin>,
	<Runtime as Chainflip>::ValidatorId,
>;

pub struct BitcoinElectionHooks;

impl
	Hooks<
		BitcoinBlockHeightTrackingES,
		BitcoinDepositChannelWitnessingES,
		BitcoinVaultDepositWitnessingES,
		BitcoinLiveness,
	> for BitcoinElectionHooks
{
	fn on_finalize(
		(block_height_tracking_identifiers, deposit_channel_witnessing_identifiers, vault_deposits_identifiers, liveness_identifiers): (
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

		BitcoinVaultDepositWitnessingES::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinVaultDepositWitnessingES,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(vault_deposits_identifiers.clone(), &chain_progress)?;

		let last_btc_block =
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
		BitcoinLiveness::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinLiveness,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(liveness_identifiers, &(current_sc_block_number, last_btc_block.block_height - 3))?;

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
		unsynchronised_state: (
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		),
		unsynchronised_settings: (
			Default::default(),
			// TODO: Write a migration to set this too.
			BWSettings { max_concurrent_elections: 15 },
			BWSettings { max_concurrent_elections: 15 },
			(),
		),
		settings: (
			Default::default(),
			Default::default(),
			Default::default(),
			LIVENESS_CHECK_DURATION,
		),
	}
}
