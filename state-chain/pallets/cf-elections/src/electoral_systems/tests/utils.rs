use cf_primitives::AuthorityCount;

use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes, ElectoralSystem},
	vote_storage::VoteStorage,
};

// Used in the unsafe_median and monotonic_median tests.
pub fn generate_votes<ES>(
	success_votes: AuthorityCount,
	authority_count: AuthorityCount,
) -> ConsensusVotes<ES>
where
	ES: ElectoralSystem<ValidatorId = ()>,
	ES::VoteStorage: VoteStorage<Properties = ()>,
	ES::VoteStorage: VoteStorage<Vote = u64>,
{
	ConsensusVotes {
		votes: (0..success_votes)
			.map(|v| ConsensusVote { vote: Some(((), v as u64)), validator_id: () })
			.chain(
				(0..(authority_count - success_votes))
					.map(|_| ConsensusVote { vote: None, validator_id: () }),
			)
			.collect(),
	}
}
