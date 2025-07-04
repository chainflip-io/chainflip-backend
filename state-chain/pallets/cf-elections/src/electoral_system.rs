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
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::vec::Vec;

use crate::{
	vote_storage::{AuthorityVote, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};

#[derive(Clone)]
pub struct ConsensusVote<ES: ElectoralSystemTypes> {
	// If the validator hasn't voted, they will get a None.
	pub vote: Option<(VotePropertiesOf<ES>, VoteOf<ES>)>,
	pub validator_id: ES::ValidatorId,
}

pub struct ConsensusVotes<ES: ElectoralSystemTypes> {
	pub votes: Vec<ConsensusVote<ES>>,
}

impl<ES: ElectoralSystemTypes> ConsensusVotes<ES> {
	// We expect that the number of votes is equal to the authority count.
	pub fn num_authorities(&self) -> AuthorityCount {
		self.votes.len() as AuthorityCount
	}

	// Returns all votes of those who actually voted.
	pub fn active_votes(self) -> Vec<VoteOf<ES>> {
		self.votes
			.into_iter()
			.filter_map(|ConsensusVote { vote, .. }| vote.map(|v| v.1))
			.collect()
	}
}

/// A trait for defining all relevant types of an electoral system.
pub trait ElectoralSystemTypes: 'static + Sized {
	type ValidatorId: Parameter + Member;

	type StateChainBlockNumber: Parameter + Member + Ord;

	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	///
	/// Note: This has the `MaybeSerializeDeserialize` bound because it appears in the genesis
	/// config of the elections pallet, which has to be (de-)serializable.
	type ElectoralUnsynchronisedState: Parameter + Member + MaybeSerializeDeserialize;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedStateMapKey: Parameter + Member;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedStateMapValue: Parameter + Member;

	/// Settings of the electoral system. These can be changed at any time by governance, and
	/// are not synchronised with elections, and therefore there is not a universal mapping from
	/// elections to these settings values. Therefore it should only be used for internal
	/// state, i.e. the engines should not consume this data.
	///
	/// Also note that if these settings are changed that will not cause election's consensus to be
	/// retested.
	///
	/// Note: This has the `MaybeSerializeDeserialize` bound because it appears in the genesis
	/// config of the elections pallet, which has to be (de-)serializable.
	type ElectoralUnsynchronisedSettings: Parameter + Member + MaybeSerializeDeserialize;

	/// Settings of the electoral system. These settings are synchronised with
	/// elections, so all engines will have a consistent view of the electoral settings to use for a
	/// given election.
	///
	/// Note: This has the `MaybeSerializeDeserialize` bound because it appears in the genesis
	/// config of the elections pallet, which has to be (de-)serializable.
	type ElectoralSettings: Parameter + Member + MaybeSerializeDeserialize + Eq;

	/// Extra data stored along with the UniqueMonotonicIdentifier as part of the
	/// ElectionIdentifier. This is used by composite electoral systems to identify which variant of
	/// election it is working with, without needing to reading in further election
	/// state/properties/etc.
	type ElectionIdentifierExtra: Parameter + Member + Copy + Eq + Ord;

	/// The properties of a single election, for example this could describe which block of the
	/// external chain the election is associated with and what needs to be witnessed.
	type ElectionProperties: Parameter + Member;

	/// Per-election state needed by the ElectoralSystem. This state is not synchronised across
	/// engines, and may change during the lifetime of a election.
	type ElectionState: Parameter + Member;

	/// A description of the validator's view of the election's topic. For example a list of all
	/// ingresses the validator has observed in the block the election is about.
	type VoteStorage: VoteStorage;

	/// This is the information that results from consensus. Typically this will be the same as the
	/// `Vote` type, but with more complex consensus models the result of an election may not be
	/// sensibly represented in the same form as a single vote.
	type Consensus: Parameter + Member + Eq;

	/// Custom parameters for `on_finalize`. Used to communicate information like the latest chain
	/// tracking block to the electoral system. While it gives more flexibility to use a generic
	/// type here, instead of an associated type, particularly as it would allow `on_finalize` to
	/// take trait instead of a specific type, I want to avoid spreading additional generics
	/// throughout the rest of the code. As an alternative, you can use dynamic dispatch (i.e.
	/// Box<dyn ...>) to achieve much the same affect.
	type OnFinalizeContext;

	/// Custom return of the `on_finalize` callback. This can be used to communicate any information
	/// you want to the caller.
	type OnFinalizeReturn;
}

#[allow(type_alias_bounds)]
pub type ElectionIdentifierOf<E: ElectoralSystemTypes> =
	ElectionIdentifier<<E as ElectoralSystemTypes>::ElectionIdentifierExtra>;
#[allow(type_alias_bounds)]
pub type AuthorityVoteOf<E: ElectoralSystemTypes> = AuthorityVote<PartialVoteOf<E>, VoteOf<E>>;
#[allow(type_alias_bounds)]
pub type VoteOf<E: ElectoralSystemTypes> =
	<<E as ElectoralSystemTypes>::VoteStorage as VoteStorage>::Vote;
#[allow(type_alias_bounds)]
pub type PartialVoteOf<E: ElectoralSystemTypes> =
	<<E as ElectoralSystemTypes>::VoteStorage as VoteStorage>::PartialVote;
#[allow(type_alias_bounds)]
pub type VoteStorageOf<E: ElectoralSystemTypes> = <E as ElectoralSystemTypes>::VoteStorage;
#[allow(type_alias_bounds)]
pub type IndividualComponentOf<E: ElectoralSystemTypes> =
	<<E as ElectoralSystemTypes>::VoteStorage as VoteStorage>::IndividualComponent;
#[allow(type_alias_bounds)]
pub type BitmapComponentOf<E: ElectoralSystemTypes> =
	<<E as ElectoralSystemTypes>::VoteStorage as VoteStorage>::BitmapComponent;
#[allow(type_alias_bounds)]
pub type VotePropertiesOf<E: ElectoralSystemTypes> =
	<<E as ElectoralSystemTypes>::VoteStorage as VoteStorage>::Properties;

/// A trait that describes a method of coming to consensus on some aspect of an external chain, and
/// how that consensus should be processed.
///
/// Implementations of this trait should *NEVER* directly access the storage of the elections
/// pallet, and only access it through the passed-in accessors.
pub trait ElectoralSystem: ElectoralSystemTypes {
	/// This is not used by the pallet, but is used to tell a validator that it should attempt
	/// to vote in a given Election. Validators are expected to call this indirectly via RPC once
	/// per state-chain block, for each active election.
	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(current_vote.is_none())
	}

	/// This is not used by the pallet, but is used to tell a validator if they should submit vote.
	/// This is a way to decrease the amount of extrinsics a validator needs to send.
	fn is_vote_needed(
		_current_vote: (VotePropertiesOf<Self>, PartialVoteOf<Self>, AuthorityVoteOf<Self>),
		_proposed_vote: (PartialVoteOf<Self>, VoteOf<Self>),
	) -> bool {
		true
	}

	/// This is called every time a vote occurs. It associates the vote with a `Properties`
	/// value.
	///
	/// You should *NEVER* update the epoch during this call. And in general updating any other
	/// state of any pallet is ill advised, and should instead be done in the 'on_finalize'
	/// function.
	fn generate_vote_properties(
		election_identifier: ElectionIdentifierOf<Self>,
		previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError>;

	/// This is called during the pallet's `on_finalize` callback, if elections aren't paused and
	/// the CorruptStorage error hasn't occurred.
	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError>;

	/// This function determines if the votes we have received form a consensus. It is called as
	/// part of the Election pallet's `on_finalize` callback when the Election's votes or state have
	/// changed since the previous call.
	///
	/// You should *NEVER* update the epoch during this call. And in general updating any other
	/// state of any pallet is ill advised, and should instead be done in the 'on_finalize'
	/// function.
	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		previous_consensus: Option<&Self::Consensus>,
		votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError>;
}

mod access {
	//! This module contains a set of interfaces used to access the details of elections. These
	//! traits abstract the underlying substrate storage items, thereby allowing ElectoralSystem's
	//! to be arbitrarily composed while still allowing each to be written in isolation, wihtout
	//! needing konwledge of how it will be composed.
	//!
	//! Also some of the storage items are lazily maintained, and so accessing them directly would
	//! provide inaccurate values. For example we don't allow access to the `Vote` details directly,
	//! which are passed directly as needed to `ElectoralSystem` trait. As the underlying storage
	//! does not strictly guarantee that all votes in the storage are from current authorities. Also
	//! this abstraction provides benefits like being able to easily test ElectoralSystem's without
	//! needing the full substrate infrastructure, and allowing cheap simulation of the existence of
	//! votes which could be useful for implementing the intended engine simulation mode.
	//!
	//! The traits in this module are split into immutable (Read) and mutable (Write) access traits,
	//! to allow the pallet to at restrict write access when it should be done, to help ensure
	//! correct ElectoralSystem implementation.

	use super::{CorruptStorageError, ElectionIdentifierOf, ElectoralSystem, ElectoralSystemTypes};

	#[cfg(test)]
	use codec::{Decode, Encode};

	/// Represents the current consensus, and how it has changed since it was last checked (i.e.
	/// 'check_consensus' was called).
	#[cfg_attr(test, derive(Clone, Debug, PartialEq, Eq, Encode, Decode))]
	pub enum ConsensusStatus<Consensus> {
		/// You did not have consensus when previously checked, but now consensus has been gained.
		Gained {
			/// If you previously had consensus, this will be `Some(...)` and will contain the most
			/// recent consensus before now.
			most_recent: Option<Consensus>,
			new: Consensus,
		},
		/// You had consensus when previously checked, but now no longer have consensus.
		Lost { previous: Consensus },
		/// You had consensus when previously checked, but the consensus has now changed.
		Changed { previous: Consensus, new: Consensus },
		/// You had consensus when previously checked, and the consensus has not changed.
		Unchanged { current: Consensus },
		/// You did not have consensus when previously checked, and still do not.
		None,
	}
	impl<Consensus> ConsensusStatus<Consensus> {
		/// Apply a closure to each `Consensus` value.
		pub fn try_map<T, E, F: Fn(Consensus) -> Result<T, E>>(
			self,
			f: F,
		) -> Result<ConsensusStatus<T>, E> {
			Ok(match self {
				ConsensusStatus::Gained { most_recent, new } => ConsensusStatus::Gained {
					most_recent: most_recent.map(&f).transpose()?,
					new: f(new)?,
				},
				ConsensusStatus::Lost { previous } =>
					ConsensusStatus::Lost { previous: f(previous)? },
				ConsensusStatus::Changed { previous, new } =>
					ConsensusStatus::Changed { previous: f(previous)?, new: f(new)? },
				ConsensusStatus::Unchanged { current } =>
					ConsensusStatus::Unchanged { current: f(current)? },
				ConsensusStatus::None => ConsensusStatus::None,
			})
		}

		/// Returns the current consensus. Returns `None` if we currently do not have consensus.
		pub fn has_consensus(self) -> Option<Consensus> {
			match self {
				ConsensusStatus::Unchanged { current: consensus } |
				ConsensusStatus::Changed { new: consensus, .. } |
				ConsensusStatus::Gained { new: consensus, .. } => Some(consensus),
				ConsensusStatus::None | ConsensusStatus::Lost { .. } => None,
			}
		}
	}

	/// A trait allowing read access to the details about a single election
	pub trait ElectionReadAccess {
		type ElectoralSystem: ElectoralSystem;

		/// Get the ElectoralSettings that are active for this election.
		fn settings(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralSettings,
			CorruptStorageError,
		>;
		fn properties(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
			CorruptStorageError,
		>;
		fn state(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
			CorruptStorageError,
		>;

		fn election_identifier(&self) -> ElectionIdentifierOf<Self::ElectoralSystem>;
	}

	/// A trait allowing write access to the details about a single election
	pub trait ElectionWriteAccess: ElectionReadAccess {
		/// Sets a new `state` value for the election. This will invalid the current Consensus, and
		/// thereby force it to be recalculated, when `check_consensus` is next called. We do this
		/// to ensure that in situations where `check_consensus` depends on the `state` that we will
		/// correctly recalculate the consensus if needed.
		fn set_state(
			&self,
			state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
		) -> Result<(), CorruptStorageError>;
		fn clear_votes(&self);
		fn delete(self);
		/// This will change the `ElectionIdentifierExtra` value of the election, and allows you to
		/// optionally change the properties. Note the `extra` must be strictly greater than the
		/// previous value of this election, this function will return `Err` if it is not. This
		/// ensures that all `Self::ElectoralSystem::ElectionIdentifierExtra` ever used by a
		/// particular election are unique. The purpose of this function to in effect allow the
		/// deletion and recreation of an election so you can change its `Properties`, while
		/// efficiently transferring the existing election's votes to the new election. The only
		/// difference is that here the elections `Settings` will not be updated to the latest. This
		/// could create a problem if you never delete elections, as old `Settings` values will be
		/// stored until any elections referencing them are deleted. Any in-flight authority votes
		/// will be invalidated by this.
		fn refresh(
			&mut self,
			new_extra: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionIdentifierExtra,
			properties: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
		) -> Result<(), CorruptStorageError>;

		/// This returns the current consensus which will always be up to date with the latest
		/// votes/state. This also returns information about the difference in the consensus between
		/// the last call to `check_consensus`.
		fn check_consensus(
			&self,
		) -> Result<
			ConsensusStatus<<Self::ElectoralSystem as ElectoralSystemTypes>::Consensus>,
			CorruptStorageError,
		>;
	}

	/// A trait allowing read access to the details about the electoral system and its elections
	pub trait ElectoralReadAccess {
		type ElectoralSystem: ElectoralSystem;
		type ElectionReadAccess: ElectionReadAccess<ElectoralSystem = Self::ElectoralSystem>;

		fn election(id: ElectionIdentifierOf<Self::ElectoralSystem>) -> Self::ElectionReadAccess;
		fn unsynchronised_settings() -> Result<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
			CorruptStorageError,
		>;
		fn unsynchronised_state() -> Result<
			<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			CorruptStorageError,
		>;
		fn unsynchronised_state_map(
			key: &<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
		) -> Result<
			Option<
				<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
			>,
			CorruptStorageError,
		>;
	}

	/// A trait allowing write access to the details about the electoral system and its elections
	pub trait ElectoralWriteAccess: ElectoralReadAccess {
		type ElectionWriteAccess: ElectionWriteAccess<ElectoralSystem = Self::ElectoralSystem>;

		fn new_election(
			extra: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionIdentifierExtra,
			properties: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionProperties,
			state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectionState,
		) -> Result<Self::ElectionWriteAccess, CorruptStorageError>;
		fn election_mut(
			id: ElectionIdentifierOf<Self::ElectoralSystem>,
		) -> Self::ElectionWriteAccess;
		fn set_unsynchronised_state(
			unsynchronised_state: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		) -> Result<(), CorruptStorageError>;

		/// Inserts or removes a value from the unsynchronised state map of the electoral system.
		fn set_unsynchronised_state_map(
			key: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
			value: Option<
				<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
			>,
		);

		/// Allows you to mutate the unsynchronised state. This is more efficient than a read
		/// (`unsynchronised_state`) and then a write (`set_unsynchronised_state`) in the case of
		/// composite ElectoralSystems, as a write from one of the sub-ElectoralSystems internally
		/// will require an additional read. Therefore this function should be preferred.
		fn mutate_unsynchronised_state<
			T,
			F: for<'a> FnOnce(
				&'a mut <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			) -> Result<T, CorruptStorageError>,
		>(
			f: F,
		) -> Result<T, CorruptStorageError>{
			let mut unsynchronised_state = Self::unsynchronised_state()?;
			let t = f(&mut unsynchronised_state)?;
			Self::set_unsynchronised_state(unsynchronised_state)?;
			Ok(t)
		}

		/// Allows you to mutate the value of the unsynchronised state map.
		fn mutate_unsynchronised_state_map<
			T,
			F: for<'a> FnOnce(
				&'a mut Option<<Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue>,
			) -> Result<T, CorruptStorageError>,
		>(
			key: <Self::ElectoralSystem as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
			f: F,
		) -> Result<T, CorruptStorageError>{
			let mut unsynchronised_state_map = Self::unsynchronised_state_map(&key)?;
			let t = f(&mut unsynchronised_state_map)?;
			Self::set_unsynchronised_state_map(key, unsynchronised_state_map);
			Ok(t)
		}
	}
}
pub use access::{
	ConsensusStatus, ElectionReadAccess, ElectionWriteAccess, ElectoralReadAccess,
	ElectoralWriteAccess,
};
