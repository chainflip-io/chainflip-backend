use cf_primitives::AuthorityCount;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::vec::Vec;

use crate::{
	vote_storage::{AuthorityVote, VoteStorage},
	CorruptStorageError, ElectionIdentifierOf,
};

/// A trait that describes a method of coming to consensus on some aspect of an external chain.
///
/// Implementations of this trait should *NEVER* directly access the storage of the election pallet,
/// and only access it through the passed-in accessors.
pub trait ElectoralSystem: 'static {
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not consumed by
	/// the engine.
	type ElectoralUnsynchronisedState: Parameter + Member + MaybeSerializeDeserialize;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not consumed by
	/// the engine.
	type ElectoralUnsynchronisedStateMapKey: Parameter + Member;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not consumed by
	/// the engine.
	type ElectoralUnsynchronisedStateMapValue: Parameter + Member;

	/// Settings of the electoral system. These can be changed at any time by governance, and
	/// are not synchronised with elections, and therefore there is not universal mapping from
	/// elections to these settings values. Therefore it should only be used for internal
	/// state, i.e. the engines should not consume this data.
	///
	/// Also note that if these settings are changed that will not cause election's to be retested.
	type ElectoralUnsynchronisedSettings: Parameter + Member + MaybeSerializeDeserialize;

	/// Settings of the electoral system. These settings are synchronised with
	/// elections, so all engines will have a consistent view of the electoral settings to use for a
	/// given election.
	type ElectoralSettings: Parameter + Member + MaybeSerializeDeserialize + Eq;

	/// Extra data stored along with the UniqueMonotonicIdentifier as part of the
	/// ElectionIdentifier. This is used by composite electoral systems to identify which variant of
	/// election it is working with.
	type ElectionIdentifierExtra: Parameter + Member + Copy + Eq + Ord;

	/// The properties of a single election, typically describing which block the election is
	/// associated with and what needs to be witnessed.
	type ElectionProperties: Parameter + Member;

	/// Per-election state needed by the ElectoralSystem. This state is not synchronised across
	/// engines, and may change during the lifetime of a election.
	type ElectionState: Parameter + Member;

	/// A description of the validator's view of the election's topic. For example a list of all
	/// ingresses the validator has observed in the block the election is about.
	type Vote: VoteStorage;

	/// This is the information that results from consensus. Typically this will be the same as the
	/// `Vote` type, but with more complex consensus models the result of an election may not be
	/// sensibly represented in the same form as a single vote.
	type Consensus: Parameter + Member + Eq;

	/// Custom parameters for `on_finalize`. Used to communicate information like the latest chain
	/// tracking block to the electoral system. While it gives more flexibility to use a generic
	/// type here, instead of an associated type, I want to avoid spreading additional generics
	/// throughout the rest of the code. As an alternative, you can use dynamic dispatch (i.e.
	/// Box<dyn ...>) to achieve much the same affect.
	type OnFinalizeContext;

	/// Custom return of the `on_finalize` callback. This can be used to communicate any information
	/// you may want to the caller.
	type OnFinalizeReturn;

	/// This is not used by the pallet, but is used to tell a validator that it should attempt
	/// to vote in a given Election. It returns the time until you should vote.
	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier_with_extra: ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(current_vote.is_none())
	}

	/// This is used in the vote extrinsic to disallow a validator from providing votes that do not
	/// pass this check.
	fn is_vote_valid<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		_partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	/// This is called every time a vote occurs. It associates the vote with a `Properties`
	/// value.
	fn vote_properties(
		election_identifier: ElectionIdentifierOf<Self>,
		previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError>;

	/// This is called during the pallet's `on_finalize` callback, if elections aren't paused and
	/// the CorruptStorage error hasn't occurred.
	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError>;

	/// This function determines if the votes we have received form a consensus. It is called as
	/// part of the Election pallet's `on_finalize` callback when the Election's votes or state have
	/// changed since the previous call.
	///
	/// You should *NEVER* update the epoch during this call.
	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_identifier: ElectionIdentifierOf<Self>,
		electoral_access: &ElectionAccess,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		previous_consensus: Option<&Self::Consensus>,
		votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError>;
}

#[allow(type_alias_bounds)]
pub type AuthorityVoteOf<E: ElectoralSystem> = AuthorityVote<
	<<E as ElectoralSystem>::Vote as VoteStorage>::PartialVote,
	<<E as ElectoralSystem>::Vote as VoteStorage>::Vote,
>;
#[allow(type_alias_bounds)]
pub type IndividualComponentOf<E: ElectoralSystem> =
	<<E as ElectoralSystem>::Vote as VoteStorage>::IndividualComponent;
#[allow(type_alias_bounds)]
pub type BitmapComponentOf<E: ElectoralSystem> =
	<<E as ElectoralSystem>::Vote as VoteStorage>::BitmapComponent;
#[allow(type_alias_bounds)]
pub type VotePropertiesOf<E: ElectoralSystem> =
	<<E as ElectoralSystem>::Vote as VoteStorage>::Properties;

pub(crate) mod access {
	//! This module contains a set of traits used to access the details of elections. Notably these
	//! don't allow access to the `Vote` details directly, which are passed directly as needed to
	//! `ElectoralSystem` trait. Their access is handled like this so it easier to simulate the
	//! existence of votes, without having to write custom implementations of these traits. This is
	//! useful to allow validators to simulate the existence of votes in pending extrinics.
	//!
	//! We also restrict access to `Vote` details as the underlying storage does not strictly
	//! guarantee that all votes are from current authorities.

	use super::{CorruptStorageError, ElectoralSystem};
	use crate::ElectionIdentifierOf;

	/// Represents the current consensus, and how it has changed since it was last checked (i.e.
	/// 'check_consensus' was called).
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

		fn settings(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystem>::ElectoralSettings,
			CorruptStorageError,
		>;
		fn properties(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
			CorruptStorageError,
		>;
		fn state(
			&self,
		) -> Result<<Self::ElectoralSystem as ElectoralSystem>::ElectionState, CorruptStorageError>;
	}

	/// A trait allowing write access to the details about a single election
	pub trait ElectionWriteAccess: ElectionReadAccess {
		fn set_state(
			&mut self,
			state: <Self::ElectoralSystem as ElectoralSystem>::ElectionState,
		) -> Result<(), CorruptStorageError>;
		fn clear_votes(&mut self);
		fn delete(self);
		/// This will change the `ElectionIdentifierExtra` of the election, and allows you to
		/// optionally change the properties. Note the `extra` must be strictöy greater than
		/// previous, this function will return `Err` if it is not. This ensures that all
		/// `ES::ElectionIdentifierExtra` ever used by a particular election are unique. The purpose
		/// of this function to in effect allow the deletion and recreation of an election so you
		/// can change its `Properties`, while efficiently transferring the existing election's vote
		/// to the new election. The only difference is that here the elections settings will not be
		/// updated to the latest. Any not yet on-chain authority votes will be invalidated by this.
		fn refresh(
			&mut self,
			extra: <Self::ElectoralSystem as ElectoralSystem>::ElectionIdentifierExtra,
			properties: <Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
		) -> Result<(), CorruptStorageError>;

		/// This returns the current consensus which will always be up to date with the latest
		/// votes/state. This also returns information about the difference in the consensus between
		/// the last call to `check_consensus`.
		fn check_consensus(
			&mut self,
		) -> Result<
			ConsensusStatus<<Self::ElectoralSystem as ElectoralSystem>::Consensus>,
			CorruptStorageError,
		>;
	}

	/// A trait allowing read access to the details about the electoral system and its elections
	pub trait ElectoralReadAccess {
		type ElectoralSystem: ElectoralSystem;
		type ElectionReadAccess<'a>: ElectionReadAccess<ElectoralSystem = Self::ElectoralSystem>
		where
			Self: 'a;

		fn election(
			&self,
			id: ElectionIdentifierOf<Self::ElectoralSystem>,
		) -> Result<Self::ElectionReadAccess<'_>, CorruptStorageError>;
		fn unsynchronised_settings(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedSettings,
			CorruptStorageError,
		>;
		fn unsynchronised_state(
			&self,
		) -> Result<
			<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedState,
			CorruptStorageError,
		>;
		fn unsynchronised_state_map(
			&self,
			key: &<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapKey,
		) -> Result<
			Option<
				<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapValue,
			>,
			CorruptStorageError,
		>;
	}

	/// A trait allowing write access to the details about the electoral system and its elections
	pub trait ElectoralWriteAccess: ElectoralReadAccess {
		type ElectionWriteAccess<'a>: ElectionWriteAccess<ElectoralSystem = Self::ElectoralSystem>
		where
			Self: 'a;

		fn new_election(
			&mut self,
			extra: <Self::ElectoralSystem as ElectoralSystem>::ElectionIdentifierExtra,
			properties: <Self::ElectoralSystem as ElectoralSystem>::ElectionProperties,
			state: <Self::ElectoralSystem as ElectoralSystem>::ElectionState,
		) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError>;
		fn election_mut(
			&mut self,
			id: ElectionIdentifierOf<Self::ElectoralSystem>,
		) -> Result<Self::ElectionWriteAccess<'_>, CorruptStorageError>;
		fn set_unsynchronised_state(
			&mut self,
			unsynchronised_state: <Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedState,
		) -> Result<(), CorruptStorageError>;
		fn set_unsynchronised_state_map(
			&mut self,
			key: <Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapKey,
			value: Option<
				<Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedStateMapValue,
			>,
		) -> Result<(), CorruptStorageError>;

		fn mutate_unsynchronised_state<
			T,
			F: for<'a> FnOnce(
				&mut Self,
				&'a mut <Self::ElectoralSystem as ElectoralSystem>::ElectoralUnsynchronisedState,
			) -> Result<T, CorruptStorageError>,
		>(
			&mut self,
			f: F,
		) -> Result<T, CorruptStorageError> {
			let mut unsynchronised_state = self.unsynchronised_state()?;
			let t = f(self, &mut unsynchronised_state)?;
			self.set_unsynchronised_state(unsynchronised_state)?;
			Ok(t)
		}
	}
}
pub use access::{
	ConsensusStatus, ElectionReadAccess, ElectionWriteAccess, ElectoralReadAccess,
	ElectoralWriteAccess,
};
