use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode};
use frame_support::{
	ensure,
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::Itertools;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct Header<BlockHash, BlockNumber> {
	pub block_height: BlockNumber,
	pub hash: BlockHash,
	pub parent_hash: BlockHash,
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct BlockHeightTrackingState<BlockHash, BlockNumber> {
	pub headers: VecDeque<Header<BlockHash, BlockNumber>>,
	pub last_safe_index: BlockNumber,
}

pub struct BlockHeightTracking<
	const SAFETY_MARGIN: u32,
	ChainBlockNumber,
	ChainBlockHash,
	Settings,
	ValidatorId,
> {
	_phantom: core::marker::PhantomData<(ChainBlockNumber, ChainBlockHash, Settings, ValidatorId)>,
}

impl<
		const SAFETY_MARGIN: u32,
		ChainBlockNumber: MaybeSerializeDeserialize + Member + Parameter + Ord + Copy,
		ChainBlockHash: MaybeSerializeDeserialize + Member + Parameter + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for BlockHeightTracking<SAFETY_MARGIN, ChainBlockNumber, ChainBlockHash, Settings, ValidatorId>
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = BlockHeightTrackingState<ChainBlockHash, ChainBlockNumber>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<Header<ChainBlockHash, ChainBlockNumber>>;
	type Consensus = Header<ChainBlockHash, ChainBlockNumber>;
	type OnFinalizeContext = ();

	// Latest safe index
	type OnFinalizeReturn = Option<ChainBlockNumber>;

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	/// Emits the most recent block that we deem safe. Thus, any downstream system can process any
	/// blocks up to this block safely.
	// How does it start up -> migrates last processed chain tracking? how do we know we want dupe
	// witnesses?
	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let election_access = ElectoralAccess::election_mut(election_identifier);
			if let Some(header) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();

				ElectoralAccess::new_election((), (), ())?;
				ElectoralAccess::mutate_unsynchronised_state(|unsynchronised_state| {
					if let Some(last_added_header) = unsynchronised_state.headers.back() {
						if header.parent_hash == last_added_header.hash {
							// we have a continuous chain
							ensure!(header.block_height > last_added_header.block_height, {
								log::error!("BlockHeightTracking: block height is not increasing, despite chain having continuous hashes");
								CorruptStorageError {}
							});
							unsynchronised_state.headers.push_back(header);
						} else {
							// we have a reorg or gaps
							println!("COuld be a reorg or gaps");
						}
					} else {
						// we have an empty chain - think about this case some more
						unsynchronised_state.headers.push_back(header);
					};

					if unsynchronised_state.headers.len() > SAFETY_MARGIN as usize {
						Ok(unsynchronised_state
							.headers
							.pop_front()
							.map(|header| header.block_height))
					} else {
						Ok(None)
					}
				})
			} else {
				Ok(None)
			}
		} else {
			// If we have no elections to process we should start one to get an updated header.
			ElectoralAccess::new_election((), (), ())?;
			Ok(None)
		}

		// if we have consensus on a block header, then header - safety is safe.
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let success_threshold = success_threshold_from_share_count(num_authorities);
		let mut active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		Ok(if num_active_votes >= success_threshold {
			// Calculating the median this way means atleast 2/3 of validators would be needed to
			// increase the calculated median.
			let (_, median_vote, _) =
				active_votes.select_nth_unstable((num_authorities - success_threshold) as usize);
			Some(median_vote.clone())
		} else {
			None
		})
	}
}
