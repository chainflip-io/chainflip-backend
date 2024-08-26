use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError,
};
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

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
pub struct Change<Identifier, Value, Settings, Hook, ValidatorId> {
	_phantom: core::marker::PhantomData<(Identifier, Value, Settings, Hook, ValidatorId)>,
}

pub trait OnChangeHook<Identifier, Value> {
	fn on_change(id: Identifier, value: Value);
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> Change<Identifier, Value, Settings, Hook, ValidatorId>
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
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem for Change<Identifier, Value, Settings, Hook, ValidatorId>
{
	type ValidatorId = ValidatorId;
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

	fn is_vote_needed(
		(_, _, current_vote): (
			VotePropertiesOf<Self>,
			<Self::Vote as VoteStorage>::PartialVote,
			AuthorityVoteOf<Self>,
		),
		(_, proposed_vote): (
			<Self::Vote as VoteStorage>::PartialVote,
			<Self::Vote as VoteStorage>::Vote,
		),
	) -> bool {
		match current_vote {
			AuthorityVoteOf::<Self>::Vote(current_vote) => current_vote != proposed_vote,
			// Could argue for either true or false. If the `PartialVote` is never reconstructed and
			// becomes invalid, then this function will be bypassed and the vote will be considered
			// needed. So false is safe, and true will likely result in unneeded voting.
			_ => false,
		}
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

#[cfg(test)]
mod test_change {

	use crate::electoral_system::{mocks::MockElectoralSystem, ConsensusStatus};

	thread_local! {
		pub static HOOK_HAS_BEEN_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
	}

	pub struct MyChangeHook;
	impl OnChangeHook<(), u64> for MyChangeHook {
		fn on_change(_id: (), _value: u64) {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(true));
		}
	}

	use super::*;
	#[test]
	fn consensus_not_possible_because_of_different_votes() {
		let mut electoral_system =
			MockElectoralSystem::<Change<(), u64, (), MyChangeHook>>::new((), (), ());
		let consensus = electoral_system
			.new_election((), ((), 1), ())
			.unwrap()
			.check_consensus(None, vec![((), 1), ((), 5), ((), 3)], 3)
			.unwrap();
		assert_eq!(consensus, None);
	}

	#[test]
	fn consensus_when_all_votes_the_same() {
		let mut electoral_system =
			MockElectoralSystem::<Change<(), u64, (), MyChangeHook>>::new((), (), ());
		let consensus = electoral_system
			.new_election((), ((), 1), ())
			.unwrap()
			.check_consensus(None, vec![((), 1), ((), 1), ((), 1)], 3)
			.unwrap();
		assert_eq!(consensus, Some(1));
	}

	#[test]
	fn if_it_consensus_then_call_hook() {
		let mut electoral_system =
			MockElectoralSystem::<Change<(), u64, (), MyChangeHook>>::new((), (), ());
		electoral_system
			.new_election((), ((), 1), ())
			.unwrap()
			.set_consensus_status(ConsensusStatus::Changed { previous: 1, new: 2 });
		electoral_system.finalize_elections(&()).unwrap();
		assert!(HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.get()));
	}
}
