// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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
