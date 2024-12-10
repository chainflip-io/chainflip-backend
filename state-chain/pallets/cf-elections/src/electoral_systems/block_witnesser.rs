use core::ops::RangeInclusive;

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, SharedDataHash,
};
use cf_chains::witness_period::BlockWitnessRange;
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::Saturating,
	Parameter,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use super::block_height_tracking::ChainProgress;

// Rather than push processing outside, we could provide an evaluation function that is called
// to determine whether to process or not. This keeps things encapsulated a little better.

// We create an election with all the channels for a particular block. Then when everyone votes
// there is nothing to witness for that election i.e. for that block then it closes the election, so
// we don't duplicate that much state at all... unless on recovery.

// How do we create elections for channels that only existed at passed state? - We manage channel
// lifetimes in the ES. Then we don't prematurely expire when we're in safe mode. The channels
// themselves can live outside the ES, but their lifetimes is managed form within the ES. We just
// need to know the id to lookup the channel and its lifetime (opened_at, closed_at).

// If there are no channels, we don't have any elections.

// safety margin???
// Double witnessing??? - should be handled by the downstream. E.g. dispatching a second boost to
// the ingress egress should be handled by ingress egress, same way it is now.

// NB: We only worry about safety margins in the on-consensus hook. Chain tracking pushes the latest
// block number, potentially with gaps which we fill. The safety is determined by the dispatching
// action, this is how we can achieve dynamic, amount based safety margins.
pub struct BlockWitnesser<Chain, BlockData, Properties, ValidatorId, OnConsensus, ElectionGenerator>
{
	_phantom: core::marker::PhantomData<(
		Chain,
		BlockData,
		Properties,
		ValidatorId,
		OnConsensus,
		ElectionGenerator,
	)>,
}

pub trait ProcessBlockData<ChainBlockNumber, BlockData> {
	/// Process the block data and return the unprocessed data. It's possible to have received data
	/// for the same block twice, in the case of a reorg. It is up to the implementor of this trait
	/// to handle this case.
	fn process_block_data(
		chain_block_number: ChainBlockNumber,
		block_data: Vec<(ChainBlockNumber, BlockData)>,
	) -> Vec<(ChainBlockNumber, BlockData)>;
}

/// Allows external/runtime/implementation to return the properties that the election should use.
/// This means each instantiation of the block witnesser can control how the properties are
/// generated, and allows for easier testing of this hook externally vs. actually creating the new
/// election inside this hook.
pub trait BlockElectionPropertiesGenerator<ChainBlockNumber, Properties> {
	fn generate_election_properties(root_block_to_witness: ChainBlockNumber) -> Properties;
}

pub type ElectionCount = u16;

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Default,
)]
pub struct BlockWitnesserSettings {
	// We don't want to start too many elections at once, as this could overload the engines.
	// e.g. If we entered safe mode for a long time and then missed 1000 blocks, without this, we
	// would start 1000 elections at the same time. Instead, we throttle the recovery.
	pub max_concurrent_elections: ElectionCount,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Default,
)]
pub struct BlockWitnesserState<ChainBlockNumber: Ord + Default, BlockData> {
	// The last block where we know that we have processed everything from....
	// what about a reorg??????
	pub last_block_election_emitted_for: ChainBlockNumber,

	// Last block we received from context.
	pub last_block_received: ChainBlockNumber,

	// The block roots (of a block range) that we received non empty block data for, but still
	// requires processing.
	// NOTE: It is possible for block data to arrive and then be partially processed. In this case,
	// the block will still be here until there is no more block data for this block root to
	// process.
	pub unprocessed_data: Vec<(ChainBlockNumber, BlockData)>,

	pub open_elections: ElectionCount,
}

impl<
		Chain: cf_chains::Chain,
		BlockData: Member + Parameter + Eq + MaybeSerializeDeserialize,
		Properties: Parameter + Member,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		BlockDataProcessor: ProcessBlockData<<Chain as cf_chains::Chain>::ChainBlockNumber, BlockData> + 'static,
		ElectionGenerator: BlockElectionPropertiesGenerator<
				<Chain as cf_chains::Chain>::ChainBlockNumber,
				Properties,
			> + 'static,
	> ElectoralSystem
	for BlockWitnesser<
		Chain,
		BlockData,
		Properties,
		ValidatorId,
		BlockDataProcessor,
		ElectionGenerator,
	>
{
	type ValidatorId = ValidatorId;
	// Store the last processed block number, number of, and the number of open elections.
	type ElectoralUnsynchronisedState =
		BlockWitnesserState<<Chain as cf_chains::Chain>::ChainBlockNumber, BlockData>;

	// We store all the unprocessed block data here, including the most recently added block data,
	// so it can be used in the OnBlockConsensus
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = BlockWitnesserSettings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	// The first item is the block number we wish to witness, the second is something else about
	// that block we want to witness. e.g. all the deposit channel addresses that are active at
	// that block.
	type ElectionProperties =
		(BlockWitnessRange<<Chain as cf_chains::Chain>::ChainBlockNumber>, Properties);
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<BlockData>;
	type Consensus = BlockData;

	// TODO: Use a specialised range type that accounts for the witness period?
	type OnFinalizeContext = ChainProgress<<Chain as cf_chains::Chain>::ChainBlockNumber>;
	type OnFinalizeReturn = ();

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		chain_progress: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		let witness_range = match chain_progress {
			ChainProgress::Reorg(range_inclusive) => range_inclusive,
			ChainProgress::Continuous(range_inclusive) => range_inclusive,
			ChainProgress::None(_) => return Ok(()), /* TODO actually we have to do something
			                                           * here... */
			ChainProgress::WaitingForFirstConsensus => return Ok(()),
		};

		ensure!(
			<Chain as cf_chains::Chain>::is_block_witness_root(*witness_range.start()) &&
				<Chain as cf_chains::Chain>::is_block_witness_root(*witness_range.end()),
			{
				log::error!(
					"Start and end of range must be block witness roots: {:?}",
					*witness_range.start()
				);
				CorruptStorageError::new()
			}
		);

		// if the start of the range is

		let BlockWitnesserState {
			mut last_block_election_emitted_for,
			last_block_received,
			mut open_elections,
			mut unprocessed_data,
		} = ElectoralAccess::unsynchronised_state()?;

		// e.g. the last block received is 20, we now have a range 19..21. This means, 19 and 20 are
		// reorg'd.
		log::info!("Last block received: {:?}", last_block_received);
		log::info!("Witness range is: {:?}", witness_range);

		// How do we handle the case where there's a reorg on the same block. We receive 4 and then
		// we get 4 again.
		if *witness_range.start() <= last_block_received {
			log::info!("Reoooooorg");
			// All ongoing elections are now invalid, we will recreate the elections, once the block
			// height witnesser passes throught those block heights again, so the engines will
			// revote.
			election_identifiers.into_iter().for_each(|election_identifier| {
				ElectoralAccess::election_mut(election_identifier).delete();
			});

			// Create a new election for each one in the range?

			for root in
				witness_range.clone().step_by(Into::<u64>::into(Chain::WITNESS_PERIOD) as usize)
			{
				log::info!("New election for root: {:?}", root);
				ElectoralAccess::new_election(
					(),
					(
						Chain::block_witness_range(root).into(),
						ElectionGenerator::generate_election_properties(root),
					),
					(),
				)?;
				last_block_election_emitted_for = root;
				open_elections = open_elections.saturating_add(1);
			}

			// NB: We do not clear any of the unprocessed data here. This is because we need to
			// prevent double dispatches. By keeping the state, if we have a reorg we can check
			// against the state in the process_block_data hook to ensure we don't double
			// dispatch.
		} else {
			// ==== No reorg case ====

			// We could have multiple elections going, for different block/ranges.
			for election_identifier in election_identifiers {
				let election_access = ElectoralAccess::election_mut(election_identifier);
				if let Some(block_data) = election_access.check_consensus()?.has_consensus() {
					let (root_block_number, _extra_properties) = election_access.properties()?;

					election_access.delete();

					open_elections = open_elections.saturating_sub(1);

					unprocessed_data.push((*root_block_number.start(), block_data));
				}
			}

			// If we haven't done any new elections, since the last run, there's not really any
			// reason to run this again, so we could probably optimise this.

			unprocessed_data =
				BlockDataProcessor::process_block_data(*witness_range.end(), unprocessed_data);

			debug_assert!(
				<Chain as cf_chains::Chain>::is_block_witness_root(last_block_election_emitted_for),
				"We only store this if it passes the original block witness root check"
			);

			let settings = ElectoralAccess::unsynchronised_settings()?;

			for range_root in (last_block_election_emitted_for
				.saturating_add(Chain::WITNESS_PERIOD)..=*witness_range.end())
				.step_by(Into::<u64>::into(Chain::WITNESS_PERIOD) as usize)
				.take(settings.max_concurrent_elections.saturating_sub(open_elections) as usize)
			{
				ElectoralAccess::new_election(
					(),
					(
						Chain::block_witness_range(range_root).into(),
						ElectionGenerator::generate_election_properties(range_root),
					),
					(),
				)?;
				last_block_election_emitted_for = range_root;
				open_elections = open_elections.saturating_add(1);
			}
		};

		ElectoralAccess::set_unsynchronised_state(BlockWitnesserState {
			last_block_received: *witness_range.end(),
			open_elections,
			last_block_election_emitted_for,
			unprocessed_data,
		})?;

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		let success_threshold = success_threshold_from_share_count(num_authorities);
		Ok(if num_active_votes >= success_threshold {
			let mut hash_to_block_data = BTreeMap::<SharedDataHash, BlockData>::new();

			let mut counts = BTreeMap::<SharedDataHash, u32>::new();
			for vote in active_votes {
				let vote_hash = SharedDataHash::of(&vote);
				hash_to_block_data.insert(vote_hash, vote.clone());
				counts.entry(vote_hash).and_modify(|count| *count += 1).or_insert(1);
			}
			counts.iter().find_map(|(vote, count)| {
				if *count >= success_threshold {
					Some(hash_to_block_data.get(vote).expect("We must insert it above").clone())
				} else {
					None
				}
			})
		} else {
			None
		})
	}
}
