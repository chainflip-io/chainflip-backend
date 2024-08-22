use crate::{
	electoral_system::{
		AuthorityVoteOf, ElectionIdentifierOf, ElectionReadAccess, ElectionWriteAccess,
		ElectoralSystem, ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError,
};
use cf_primitives::AuthorityCount;
use cf_utilities::{all_same, success_threshold_from_share_count};
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::vec::Vec;

/// This electoral system detects if something occurred or not. Voters simply vote if something
/// happened, and if they haven't seen it happen, they don't vote.
pub struct EgressSuccess<Identifier, Value, Settings, Hook> {
	_phantom: core::marker::PhantomData<(Identifier, Value, Settings, Hook)>,
}

pub trait OnEgressSuccess<Identifier, Value> {
	fn on_egress_success(id: Identifier, value: Value);
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnEgressSuccess<Identifier, Value> + 'static,
	> EgressSuccess<Identifier, Value, Settings, Hook>
{
	pub fn watch_for_egress<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		identifier: Identifier,
	) -> Result<(), CorruptStorageError> {
		electoral_access.new_election((), identifier, ())?;
		Ok(())
	}
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnEgressSuccess<Identifier, Value> + 'static,
	> ElectoralSystem for EgressSuccess<Identifier, Value, Settings, Hook>
{
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = Identifier;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<Value>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if let Some(egress_data) = election_access.check_consensus()?.has_consensus() {
				let identifier = election_access.properties()?;
				election_access.delete();
				Hook::on_egress_success(identifier, egress_data);
			}
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let votes_count = votes.len();
		Ok(
			if votes_count != 0 &&
				votes_count >= success_threshold_from_share_count(authorities) as usize
			{
				all_same(votes.into_iter().map(|(_, vote)| vote))
			} else {
				None
			},
		)
	}
}
