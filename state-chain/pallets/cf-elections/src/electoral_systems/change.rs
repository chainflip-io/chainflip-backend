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
pub struct NonceWitnessing<Identifier, Value, Slot, Settings, Hook, ValidatorId> {
	_phantom: core::marker::PhantomData<(Identifier, Value, Slot, Settings, Hook, ValidatorId)>,
}

pub trait OnChangeHook<Identifier, Value> {
	fn on_change(id: Identifier, value: Value);
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Slot: Member + Parameter + Eq + Ord + Copy + Default,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> NonceWitnessing<Identifier, Value, Slot, Settings, Hook, ValidatorId>
{
	pub fn watch_for_change<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		identifier: Identifier,
		previous_value: Value,
	) -> Result<(), CorruptStorageError> {
		// TODO: we can probably switch to expect, after initialization we always expect to have a
		// value here!
		let previous_slot =
			electoral_access.unsynchronised_state_map(&identifier)?.unwrap_or_default();
		electoral_access.new_election((), (identifier, previous_value, previous_slot), ())?;
		Ok(())
	}
}
impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Slot: Member + Parameter + Eq + Ord + Copy + Default,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem for NonceWitnessing<Identifier, Value, Slot, Settings, Hook, ValidatorId>
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = Identifier;
	type ElectoralUnsynchronisedStateMapValue = Slot;
	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (Identifier, Value, Slot);
	type ElectionState = ();
	type Vote = vote_storage::nonce::NonceStorage<Value, Slot>;
	type Consensus = (Value, Slot);
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

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

	fn is_vote_valid<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifierOf<Self>,
		election_access: &ElectionAccess,
		partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<bool, CorruptStorageError> {
		let (_, previous_value, previous_slot) = election_access.properties()?;
		Ok(partial_vote.value != previous_value && partial_vote.slot > previous_slot)
	}
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if let Some((value, slot)) = election_access.check_consensus()?.has_consensus() {
				let (identifier, previous_value, previous_slot) = election_access.properties()?;
				if previous_value != value && slot > previous_slot {
					election_access.delete();
					Hook::on_change(identifier.clone(), value);
					electoral_access.set_unsynchronised_state_map(identifier, Some(slot))?;
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
			for vote in active_votes.clone() {
				counts.entry(vote.value).and_modify(|count| *count += 1).or_insert(1);
			}
			let mut active_slots = active_votes.iter().map(|elem| elem.slot).collect::<Vec<_>>();
			counts.iter().find_map(|(vote, count)| {
				if *count >= success_threshold {
					let (_, median_vote, _) =
						{ active_slots.select_nth_unstable((success_threshold - 1) as usize) };
					Some((vote.clone(), *median_vote))
				} else {
					None
				}
			})
		} else {
			None
		})
	}
}
