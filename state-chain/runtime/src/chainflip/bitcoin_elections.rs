use crate::{BitcoinIngressEgress, Runtime};
use cf_chains::{btc, Bitcoin};
use cf_traits::Chainflip;
use log::info;

use cf_chains::instances::BitcoinInstance;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_height_tracking::{self, BlockHeightTracking},
		block_witnesser::{BlockElectionPropertiesGenerator, BlockWitnesser, ProcessBlockData},
		composite::{
			// tuple_1_impls::{DerivedElectoralAccess, Hooks},
			tuple_2_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf, RunnerStorageAccess,
};

use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
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

pub type BitcoinDepositChannelWitnessing = BlockWitnesser<
	Bitcoin,
	Vec<DepositWitness<Bitcoin>>,
	Vec<DepositChannelDetails<Runtime, BitcoinInstance>>,
	<Runtime as Chainflip>::ValidatorId,
	BitcoinDepositChannelWitessingProcessor,
	BitcoinDepositChannelWitnessingGenerator,
>;

pub type BitcoinBlockHeightTracking =
	BlockHeightTracking<6, btc::BlockNumber, btc::Hash, (), <Runtime as Chainflip>::ValidatorId>;

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
		witnesses: Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)>,
	) -> Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)> {
		let witnesses = witnesses
			.into_iter()
			.map(|(block_number, deposits)| {
				log::info!(
					"Processing block number: {}, got {} deposits",
					block_number,
					deposits.len()
				);
				// Check if the block number is the current block number
				// If it is, then we can process the deposits
				// If it is not, then we can store the deposits for later processing
				(block_number, deposits)
			})
			.collect::<Vec<_>>();

		info!("Processing block number: {}, got {} deposits", current_block, witnesses.len());

		witnesses

		// when is it safe to expire a channel? when the block number is beyond their expiry? but
		// what if we're at block 40 it expires at block 39 and then we reorg back to block 36. It
		// will already be expired.

		// Channel expiry here should be viewed as, from what block should it be included in an
		// election. The recycle height is the moment from which if we were to have reached it, a
		// reorg back to before the expiry would cause a bug - let's assert on this assumption
		// somewhere.
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
		let new_block_range = BitcoinBlockHeightTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				BitcoinBlockHeightTracking,
				RunnerStorageAccess<Runtime, BitcoinInstance>,
			>,
		>(block_height_tracking_identifiers, &())?;

		if let Some(new_block_range) = new_block_range {
			log::info!("BitcoinElectionHooks::on_finalize: {:?}", new_block_range);
			BitcoinDepositChannelWitnessing::on_finalize::<
				DerivedElectoralAccess<
					_,
					BitcoinDepositChannelWitnessing,
					RunnerStorageAccess<Runtime, BitcoinInstance>,
				>,
			>(deposit_channel_witnessing_identifiers, &new_block_range)?;
		}

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
		unsynchronised_settings: (Default::default(), Default::default()),
		settings: (Default::default(), Default::default()),
	}
}
