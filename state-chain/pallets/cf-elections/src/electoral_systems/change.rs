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

/// This electoral system detects if a value changes. The SC can request that it detects if a
/// particular value, the instance of which is specified by an identifier, has changed from some
/// specified value. Once a change is detected and gains consensus the hook is called and the system
/// will stop trying to detect changes for that identifier.
///
/// `Settings` can be used by governance to provide information to authorities about exactly how
/// they should `vote`.
///
/// Authorities only need to vote if their observed value is different than the one specified in the
/// `ElectionProperties`.
pub struct Change<Identifier, Value, Settings, Hook> {
	_phantom: core::marker::PhantomData<(Identifier, Value, Settings, Hook)>,
}

pub trait OnChangeHook<Identifier, Value> {
	fn on_change(id: Identifier, value: Value);
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
	> Change<Identifier, Value, Settings, Hook>
{
	pub fn watch_for_change<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		identifier: Identifier,
		previous_value: Value,
	) -> Result<(), CorruptStorageError> {
		electoral_access.new_election((), (identifier, previous_value), ())?;
		Ok(())
	}
}
impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
	> ElectoralSystem for Change<Identifier, Value, Settings, Hook>
{
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (Identifier, Value);
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<Value>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

	fn vote_properties(
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
			if let Some(value) = election_access.check_consensus()?.has_consensus() {
				let (identifier, previous_value) = election_access.properties()?;
				if previous_value != value {
					election_access.delete();
					Hook::on_change(identifier, value);
				}
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
