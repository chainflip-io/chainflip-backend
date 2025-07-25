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

use sp_std::collections::btree_set::BTreeSet;

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVote, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralSystemTypes, ElectoralWriteAccess,
		PartialVoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError,
};
use cf_primitives::AuthorityCount;
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};

use itertools::Itertools;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub struct Liveness<ChainBlockNumber, ChainBlockHash, Hook, ValidatorId, StateChainBlockNumber> {
	_phantom: core::marker::PhantomData<(
		ChainBlockNumber,
		ChainBlockHash,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	)>,
}

pub trait OnCheckComplete<ValidatorId> {
	fn on_check_complete(validator_ids: BTreeSet<ValidatorId>);
}

impl<
		ChainBlockNumber: Member + Parameter + Eq + From<u64> + Into<u64> + Copy,
		ChainBlockHash: Member + Parameter + Eq + Ord,
		Hook: OnCheckComplete<ValidatorId> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystemTypes
	for Liveness<ChainBlockNumber, ChainBlockHash, Hook, ValidatorId, StateChainBlockNumber>
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	// How many SC blocks to wait before we consider the election complete.
	type ElectoralSettings = StateChainBlockNumber;
	type ElectionIdentifierExtra = ();

	// The external chain block number that the engines will get the hash for.
	type ElectionProperties = ChainBlockNumber;

	// The SC block number that we started the election at.
	type ElectionState = StateChainBlockNumber;
	type VoteStorage = vote_storage::bitmap::Bitmap<ChainBlockHash>;
	type Consensus = BTreeSet<Self::ValidatorId>;
	// The current SC block number, and the current chain tracking height.
	type OnFinalizeContext = (StateChainBlockNumber, ChainBlockNumber);
	type OnFinalizeReturn = ();
}

impl<
		ChainBlockNumber: Member + Parameter + Eq + From<u64> + Into<u64> + Copy,
		ChainBlockHash: Member + Parameter + Eq + Ord,
		Hook: OnCheckComplete<ValidatorId> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member
			+ Parameter
			+ Ord
			+ MaybeSerializeDeserialize
			+ Into<u32>
			+ frame_support::sp_runtime::Saturating
			+ Copy,
	> ElectoralSystem
	for Liveness<ChainBlockNumber, ChainBlockHash, Hook, ValidatorId, StateChainBlockNumber>
{
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(current_vote.is_none())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		(current_sc_block, current_chain_tracking_number): &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		fn block_number_to_check<ChainBlockNumber: Into<u64> + From<u64>>(
			current_sc_block: u32,
			current_chain_tracking_number: ChainBlockNumber,
		) -> ChainBlockNumber {
			use nanorand::{Rng, WyRand};
			let height: u64 = current_chain_tracking_number.into();

			// 100 seems a reasonable number for all chains, fast or slow, most running nodes will
			// have at least 100 blocks worth of state for any particular chain, while still
			// providing enough variation.
			const RANGE: u64 = 100;
			WyRand::new_seed(current_sc_block as u64)
				.generate_range((height.saturating_sub(RANGE))..height)
				.into()
		}

		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let election_access = ElectoralAccess::election_mut(election_identifier);

			// Is the block the election started at + the duration we want the check to stay open
			// for less than or equal to the current SC block?
			if election_access.state()?.saturating_add(election_access.settings()?) <=
				*current_sc_block
			{
				if let Some(bad_validators) = election_access.check_consensus()?.has_consensus() {
					if !bad_validators.is_empty() {
						Hook::on_check_complete(bad_validators);
					}
				}
				election_access.delete();
				ElectoralAccess::new_election(
					(),
					block_number_to_check(
						(*current_sc_block).into(),
						*current_chain_tracking_number,
					),
					*current_sc_block,
				)?;
			}
		} else {
			ElectoralAccess::new_election(
				(),
				block_number_to_check((*current_sc_block).into(), *current_chain_tracking_number),
				*current_sc_block,
			)?;
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let success_threshold = success_threshold_from_share_count(num_authorities);

		let mut grouped_votes = BTreeMap::new();
		for ConsensusVote { vote, validator_id } in consensus_votes.votes {
			grouped_votes
				.entry(vote.map(|v| v.1))
				.or_insert_with(Vec::new)
				.push(validator_id);
		}

		let (consensus_validators, non_consensus_validators): (Vec<_>, Vec<_>) =
			grouped_votes.into_iter().partition(|(_, validator_ids)| {
				validator_ids.len() as AuthorityCount >= success_threshold
			});

		Ok(if let Some((Some(_block_hash), _)) = consensus_validators.first() {
			// If we have consensus on a value then we punish all validators that didn't vote for
			// that value.
			Some(
				non_consensus_validators
					.into_iter()
					.flat_map(|(_, validator_ids)| validator_ids)
					.collect(),
			)
		} else {
			None
		})
	}
}
