use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::vec::Vec;

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectoralSystemTypes, PartialVoteOf,
		VoteOf, VotePropertiesOf,
	},
	vote_storage::{AuthorityVote, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};

use crate::electoral_system::ConsensusStatus;

// #[allow(type_alias_bounds)]
// pub type ElectionIdentifierOf<E: ElectoralSystemRunner> =
// 	ElectionIdentifier<<E as ElectoralSystemRunner>::ElectionIdentifierExtra>;

// #[allow(type_alias_bounds)]
// pub type AuthorityVoteOf<E: ElectoralSystemRunner> = AuthorityVote<
// 	PartialVoteOf<<E as ElectoralSystemRunner>>,
// 	VoteOf<<E as ElectoralSystemRunner>>,
// >;
// #[allow(type_alias_bounds)]
// pub type IndividualComponentOf<E: ElectoralSystemRunner> =
// 	<<E as ElectoralSystemRunner>::Vote as VoteStorage>::IndividualComponent;
// #[allow(type_alias_bounds)]
// pub type BitmapComponentOf<E: ElectoralSystemRunner> =
// 	<<E as ElectoralSystemRunner>::Vote as VoteStorage>::BitmapComponent;
// #[allow(type_alias_bounds)]
// pub type VotePropertiesOf<E: ElectoralSystemRunner> =
// 	<<E as ElectoralSystemRunner>::Vote as VoteStorage>::Properties;

// pub struct ConsensusVote<ES: ElectoralSystemRunner> {
// 	// If the validator hasn't voted, they will get a None.
// 	pub vote: Option<(VotePropertiesOf<ES>, VoteOf<ES>)>,
// 	pub validator_id: ES::ValidatorId,
// }

// pub struct ConsensusVotes<ES: ElectoralSystemRunner> {
// 	pub votes: Vec<ConsensusVote<ES>>,
// }

// #[cfg(test)]
// impl<ES: ElectoralSystemRunner> ConsensusVotes<ES> {
// 	pub fn active_votes(self) -> Vec<VoteOf<ES>> {
// 		self.votes
// 			.into_iter()
// 			.filter_map(|ConsensusVote { vote, .. }| vote.map(|v| v.1))
// 			.collect()
// 	}
// }

/// A trait used to define a runner of electoral systems. An object implementing this trait is
/// injected into an elections pallet, which then executes the necessary logic to run each electoral
/// system's logic.
/// The primary implementation of this trait is the `CompositeRunner`. This should be the *only*
/// implementation of this trait. This ensures that the storage and access is consistent across all
/// electoral systems. i.e. we always wrap the storage types. Which leads to consistent and
/// therefore simpler migration logic.
pub trait ElectoralSystemRunner:
	ElectoralSystemTypes<OnFinalizeContext = (), OnFinalizeReturn = ()>
{
	/// This is not used by the pallet, but is used to tell a validator that it should attempt
	/// to vote in a given Election. Validators are expected to call this indirectly via RPC once
	/// per state-chain block, for each active election.
	fn is_vote_desired(
		_election_identifier_with_extra: ElectionIdentifierOf<Self>,
		current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
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

	/// This is used in the vote extrinsic to disallow a validator from providing votes that do not
	/// pass this check. It is guaranteed that any vote values provided to
	/// `generate_vote_properties`, or `check_consensus` have past this check.
	///
	/// We only pass the `PartialVote` into the validity check, instead of the `AuthorityVote` or
	/// `Vote`, to ensure the check's logic is consistent regardless of if the authority provides a
	/// `Vote` or `PartialVote`. If the check was different depending on if the authority voted with
	/// a `PartialVote` or `Vote`, then check only guarantees of the intersection of the two
	/// variations.
	///
	/// You should *NEVER* update the epoch during this call. And in general updating any other
	/// state of any pallet is ill advised, and should instead be done in the 'on_finalize'
	/// function.
	fn is_vote_valid(
		_election_identifier: ElectionIdentifierOf<Self>,
		_partial_vote: &PartialVoteOf<Self>,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
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
	fn on_finalize(
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
	) -> Result<(), CorruptStorageError>;

	/// This function determines if the votes we have received form a consensus. It is called as
	/// part of the Election pallet's `on_finalize` callback when the Election's votes or state have
	/// changed since the previous call.
	///
	/// You should *NEVER* update the epoch during this call. And in general updating any other
	/// state of any pallet is ill advised, and should instead be done in the 'on_finalize'
	/// function.
	#[allow(clippy::type_complexity)]
	fn check_consensus(
		election_identifier: ElectionIdentifierOf<Self>,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		previous_consensus: Option<&Self::Consensus>,
		votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError>;
}

use crate::UniqueMonotonicIdentifier;

/// A trait allowing access to a storage layer for electoral sytem runners.
// TODO: rename
pub trait RunnerStorageAccessTrait {
	type ElectoralSystemRunner: ElectoralSystemRunner;

	fn electoral_settings_for_election(
		unique_monotonic_identifier: UniqueMonotonicIdentifier,
	) -> Result<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
		CorruptStorageError,
	>;
	fn election_properties(
		election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
	) -> Result<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
		CorruptStorageError,
	>;
	fn election_state(
		unique_monotonic_identifier: UniqueMonotonicIdentifier,
	) -> Result<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
		CorruptStorageError,
	>;

	/// Sets a new `state` value for the election. This will invalid the current Consensus, and
	/// thereby force it to be recalculated, when `check_consensus` is next called. We do this
	/// to ensure that in situations where `check_consensus` depends on the `state` that we will
	/// correctly recalculate the consensus if needed.
	fn set_election_state(
		unique_monotonic_identifier: UniqueMonotonicIdentifier,
		state: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
	) -> Result<(), CorruptStorageError>;

	// Clear the votes of a particular election
	fn clear_election_votes(unique_monotonic_identifier: UniqueMonotonicIdentifier);

	fn delete_election(
		composite_election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
	);
	/// This will change the `ElectionIdentifierExtra` value of the election, and allows you to
	/// optionally change the properties. Note the `extra` must be strictly greater than the
	/// previous value of this election, this function will return `Err` if it is not. This
	/// ensures that all `Self::ElectoralSystemRunner::ElectionIdentifierExtra` ever used by a
	/// particular election are unique. The purpose of this function to in effect allow the
	/// deletion and recreation of an election so you can change its `Properties`, while
	/// efficiently transferring the existing election's votes to the new election. The only
	/// difference is that here the elections `Settings` will not be updated to the latest. This
	/// could create a problem if you never delete elections, as old `Settings` values will be
	/// stored until any elections referencing them are deleted. Any in-flight authority votes
	/// will be invalidated by this.
	fn refresh_election(
		election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
		new_extra: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<(), CorruptStorageError>;

	/// This returns the current consensus which will always be up to date with the latest
	/// votes/state. This also returns information about the difference in the consensus between
	/// the last call to `check_consensus`.
	fn check_election_consensus(
		election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
	) -> Result<
		ConsensusStatus<<Self::ElectoralSystemRunner as ElectoralSystemTypes>::Consensus>,
		CorruptStorageError,
	>;

	fn unsynchronised_settings() -> Result<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		CorruptStorageError,
	>;
	fn unsynchronised_state() -> Result<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		CorruptStorageError,
	>;
	fn unsynchronised_state_map(
		key: &<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
	) -> Option<
		<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
	>;

	fn new_election(
		extra: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra,
		properties: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
		state: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
	) -> Result<ElectionIdentifierOf<Self::ElectoralSystemRunner>, CorruptStorageError>;

	fn set_unsynchronised_state(
		unsynchronised_state: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
	);

	/// Inserts or removes a value from the unsynchronised state map of the electoral system.
	fn set_unsynchronised_state_map(
		key: <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
		value: Option<
				<Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
			>,
	);

	/// Allows you to mutate the unsynchronised state. This is more efficient than a read
	/// (`unsynchronised_state`) and then a write (`set_unsynchronised_state`) in the case of
	/// composite ElectoralSystems, as a write from one of the sub-ElectoralSystems internally
	/// will require an additional read. Therefore this function should be preferred.
	fn mutate_unsynchronised_state<
		T,
		F: for<'a> FnOnce(
			&Self,
			&'a <Self::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		) -> Result<T, CorruptStorageError>,
	>(
		&self,
		f: F,
	) -> Result<T, CorruptStorageError> {
		let mut unsynchronised_state = Self::unsynchronised_state()?;
		let t = f(self, &mut unsynchronised_state)?;
		Self::set_unsynchronised_state(unsynchronised_state);
		Ok(t)
	}
}
