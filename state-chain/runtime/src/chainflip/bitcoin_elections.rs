use crate::{BitcoinIngressEgress, Runtime};
use cf_chains::{btc, Bitcoin};
use cf_traits::Chainflip;
use log::info;

use cf_chains::instances::BitcoinInstance;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_height_tracking::{
			self,
			state_machine_es::DsmElectoralSystem,
			BlockHeightTrackingConsensus, BlockHeightTrackingDSM,
		},
		block_witnesser::{BlockElectionPropertiesGenerator, BlockWitnesser, BlockWitnesserSettings, ProcessBlockData},
		composite::{
			tuple_2_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf, RunnerStorageAccess,
};

use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness, ProcessedUpTo, WitnessSafetyMargin};
use scale_info::TypeInfo;

use sp_runtime::Either;
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

pub type BitcoinDepositChannelWitnessing = BlockWitnesser<
	Bitcoin,
	Vec<DepositWitness<Bitcoin>>,
	Vec<DepositChannelDetails<Runtime, BitcoinInstance>>,
	<Runtime as Chainflip>::ValidatorId,
	BitcoinDepositChannelWitessingProcessor,
	BitcoinDepositChannelWitnessingGenerator,
>;

pub type BitcoinBlockHeightTracking = DsmElectoralSystem<
	BlockHeightTrackingDSM<6, btc::BlockNumber, btc::Hash>,
	<Runtime as Chainflip>::ValidatorId,
	(),
	BlockHeightTrackingConsensus<btc::BlockNumber, btc::Hash>,
>;

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
		>(block_height_tracking_identifiers, &())?;

		let chain_progress = match chain_progress {
			Either::Left(x) => x,
			Either::Right(x) => x,
		};

		log::info!("BitcoinElectionHooks::on_finalize: {:?}", chain_progress);
		BitcoinDepositChannelWitnessing::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinDepositChannelWitnessing,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(deposit_channel_witnessing_identifiers, &chain_progress)?;

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
			BlockWitnesserSettings { max_concurrent_elections: 5 },
		),
		settings: (Default::default(), Default::default()),
	}
}