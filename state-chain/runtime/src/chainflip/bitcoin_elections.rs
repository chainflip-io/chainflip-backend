use crate::Runtime;
use cf_chains::{btc, Bitcoin};
use cf_traits::Chainflip;
use pallet_cf_ingress_egress::DepositChannelDetails;

use cf_chains::instances::BitcoinInstance;

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_witnesser::{BlockElectionPropertiesGenerator, BlockWitnesser, ProcessBlockData},
		composite::{tuple_1_impls::Hooks, CompositeRunner},
	},
	CorruptStorageError, ElectionIdentifier, RunnerStorageAccess,
};

use pallet_cf_ingress_egress::DepositWitness;
use scale_info::TypeInfo;

use sp_std::{vec, vec::Vec};

pub type BitcoinElectoralSystemRunner = CompositeRunner<
	(BitcoinDepositChannelWitnessing,),
	<Runtime as Chainflip>::ValidatorId,
	RunnerStorageAccess<Runtime, BitcoinInstance>,
	BitcoinElectionHooks,
>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OpenChannelDetails<ChainBlockNumber> {
	pub open_block: ChainBlockNumber,
	pub close_block: ChainBlockNumber,
}

type BitcoinDepositChannelWitnessing = BlockWitnesser<
	Bitcoin,
	Vec<DepositWitness<Bitcoin>>,
	Vec<DepositChannelDetails<Runtime, BitcoinInstance>>,
	<Runtime as Chainflip>::ValidatorId,
	BitcoinDepositChannelWitessingProcessor,
	BitcoinDepositChannelWitnessingGenerator,
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
		// Get addresses for this block, and any that have expired after this block.

		// then generate election with addresses for this block
		// then trigger exipry for any addresses that have expired after this block

		// The fetching of valid addresses can be done inside the ingress-egress pallet where they
		// are stored. maybe the expiry too.
		// let deposit_channels_for_block = BTreeMap::new();
		log::info!("Generating election for block number: {}", block_witness_root);

		vec![]
	}
}

pub struct BitcoinDepositChannelWitessingProcessor;

impl ProcessBlockData<btc::BlockNumber, Vec<DepositWitness<Bitcoin>>>
	for BitcoinDepositChannelWitessingProcessor
{
	fn process_block_data(
		_current_block: btc::BlockNumber,
		witnesses: Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)>,
	) -> Vec<(btc::BlockNumber, Vec<DepositWitness<Bitcoin>>)> {
		witnesses
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
			.collect()

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

impl Hooks<BitcoinDepositChannelWitnessing> for BitcoinElectionHooks {
	fn on_finalize(
		(deposit_channel_witnessing_identifiers,): (
			Vec<
				ElectionIdentifier<
					<BitcoinDepositChannelWitnessing as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		log::info!(
			"BitcoinElectionHooks::on_finalize: {:?}",
			deposit_channel_witnessing_identifiers
		);
		todo!()
	}
}

// Channel expiry:
// We need to process elections in order, even after a safe mode pause. This is to ensure channel
// expiry is done correctly. During safe mode pause, we could get into a situation where the current
// state suggests that a channel is expired, but at the time of a previous block which we have not
// yet processed, the channel was not expired.
