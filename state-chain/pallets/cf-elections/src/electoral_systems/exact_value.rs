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

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralSystemTypes, ElectoralWriteAccess,
		PartialVoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError,
};
use cf_runtime_utilities::log_or_panic;
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// This electoral system detects if something occurred or not. Voters simply vote if something
/// happened, and if they haven't seen it happen, they don't vote.
#[allow(clippy::type_complexity)]
pub struct ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber> {
	_phantom: core::marker::PhantomData<(
		Identifier,
		Value,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	)>,
}

pub trait ExactValueHook<Identifier, Value> {
	type StorageKey: Parameter + Member;
	type StorageValue: Parameter + Member;

	/// Called when a consensus is reached. The hook can return a tuple of `(Self::StorageKey,
	/// Self::StorageValue)` to be stored in the unsynchronised state map.
	fn on_consensus(id: Identifier, value: Value)
		-> Option<(Self::StorageKey, Self::StorageValue)>;
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	pub fn witness_exact_value<
		ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static,
	>(
		identifier: Identifier,
	) -> Result<(), CorruptStorageError> {
		ElectoralAccess::new_election((), identifier, ())?;
		Ok(())
	}

	pub fn take_election_result<
		ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static,
	>(
		id: Hook::StorageKey,
	) -> Option<Hook::StorageValue> {
		ElectoralAccess::mutate_unsynchronised_state_map(id.clone(), |storage| {
			Ok(if let Some((counter, value)) = storage.take() {
				if counter > 1 {
					let _ = storage.insert((counter - 1, value.clone()));
				}
				Some(value)
			} else {
				None
			})
		})
		.unwrap_or_else(|_| {
			log_or_panic!("Failed to get result for election {:?} due to corrupted storage", id);
			None
		})
	}
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystemTypes
	for ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = Hook::StorageKey;
	type ElectoralUnsynchronisedStateMapValue = (u16, Hook::StorageValue);

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = Identifier;
	type ElectionState = ();
	type VoteStorage = vote_storage::bitmap::Bitmap<Value>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_needed(
		(_, current_partial_vote, _): (
			VotePropertiesOf<Self>,
			PartialVoteOf<Self>,
			AuthorityVoteOf<Self>,
		),
		(proposed_partial_vote, _): (PartialVoteOf<Self>, crate::VoteOf<Self>),
	) -> bool {
		current_partial_vote != proposed_partial_vote
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let election_access = ElectoralAccess::election_mut(election_identifier);
			let identifier = election_access.properties()?;
			if let Some(witnessed_value) = election_access.check_consensus()?.has_consensus() {
				if let Some((key, value)) = Hook::on_consensus(identifier, witnessed_value) {
					ElectoralAccess::mutate_unsynchronised_state_map(key, |storage| {
						if let Some((old_counter, _)) = storage.replace((1, value)) {
							if let Some((counter, _)) = storage.as_mut() {
								*counter = old_counter + 1;
							};
						}
						Ok(())
					})?;
				}
				election_access.delete();
			}
		}

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
			let mut counts = BTreeMap::new();
			for vote in active_votes {
				counts.entry(vote).and_modify(|count| *count += 1).or_insert(1);
			}
			counts.iter().find_map(|(vote, count)| {
				if *count >= success_threshold {
					Some(vote.clone())
				} else {
					None
				}
			})
		} else {
			None
		})
	}
}
