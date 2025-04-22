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
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralSystemTypes, ElectoralWriteAccess, PartialVoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError, ElectionIdentifier,
};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::Itertools;
use sp_std::vec::Vec;

pub trait UpdateFeeHook<Value> {
	fn update_fee(fee: Value);
}

/// This electoral system calculates the median of all the authorities votes and stores the latest
/// median in the `ElectoralUnsynchronisedState`. Each time consensus is gained, everyone is asked
/// to revote, to provide a new updated value. *IMPORTANT*: This is not the most secure method as
/// only 1/3 is needed to change the median's value arbitrarily, even though we do use the same
/// median calculation elsewhere. For something more secure see `MonotonicMedian`.
///
/// `Settings` can be used by governance to provide information to authorities about exactly how
/// they should `vote`.
pub struct UnsafeMedian<Value, UnsynchronisedSettings, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	_phantom: core::marker::PhantomData<(
		Value,
		UnsynchronisedSettings,
		Settings,
        Hook,
		ValidatorId,
		StateChainBlockNumber,
	)>,
}
impl<
		Value: Member + Parameter + MaybeSerializeDeserialize + Ord + BenchmarkValue,
		UnsynchronisedSettings: Member + Parameter + MaybeSerializeDeserialize,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: UpdateFeeHook<Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
        StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
> ElectoralSystem for UnsafeMedian<Value, UnsynchronisedSettings, Settings, ValidatorId, StateChainBlockNumber>
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;

	type ElectoralUnsynchronisedState = Value;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = UnsynchronisedSettings;
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	type VoteStorage =
		vote_storage::individual::Individual<(), vote_storage::individual::shared::Shared<Value>>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();
}

impl<
		Value: Member + Parameter + MaybeSerializeDeserialize + Ord + BenchmarkValue,
		UnsynchronisedSettings: Member + Parameter + MaybeSerializeDeserialize,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
        Hook: UpdateFeeHook<Value> + 'static,
        ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for UnsafeMedian<Value, UnsynchronisedSettings, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

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
			if let Some(consensus) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();
				ElectoralAccess::set_unsynchronised_state(consensus.clone())?;
				Hook::update_fee(consensus);
			}
		} else {
			ElectoralAccess::new_election((), (), ())?;
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = votes.num_authorities();
		let mut active_votes = votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		Ok(
			if num_active_votes != 0 &&
				num_active_votes >= success_threshold_from_share_count(num_authorities)
			{
				let (_, median_vote, _) =
					active_votes.select_nth_unstable(((num_active_votes - 1) / 2) as usize);
				Some(median_vote.clone())
			} else {
				None
			},
		)
	}
}
