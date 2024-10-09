use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, SharedDataHash,
};
use cf_runtime_utilities::log_or_panic;
use cf_utilities::{success_threshold_from_share_count, threshold_from_share_count};
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

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
pub struct MonotonicChange<Identifier, Value, BlockHeight, Settings, Hook, ValidatorId> {
	_phantom:
		core::marker::PhantomData<(Identifier, Value, BlockHeight, Settings, Hook, ValidatorId)>,
}

pub trait OnChangeHook<Identifier, Value> {
	fn on_change(id: Identifier, value: Value);
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		BlockHeight: Member + Parameter + Eq + Ord + Copy + Default,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> MonotonicChange<Identifier, Value, BlockHeight, Settings, Hook, ValidatorId>
{
	pub fn watch_for_change<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		identifier: Identifier,
		previous_value: Value,
	) -> Result<(), CorruptStorageError> {
		let previous_block_height =
			electoral_access.unsynchronised_state_map(&identifier)?.unwrap_or_default();
		electoral_access.new_election(
			(),
			(identifier, previous_value, previous_block_height),
			(),
		)?;
		Ok(())
	}
}
impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		BlockHeight: Member + Parameter + Eq + Ord + Copy + Default,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem for MonotonicChange<Identifier, Value, BlockHeight, Settings, Hook, ValidatorId>
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = Identifier;
	type ElectoralUnsynchronisedStateMapValue = BlockHeight;
	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (Identifier, Value, BlockHeight);
	type ElectionState = ();
	type Vote = vote_storage::change::MonotonicChange<Value, BlockHeight>;
	type Consensus = (Value, BlockHeight);
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
		Ok(partial_vote.value != SharedDataHash::of(&previous_value) &&
			partial_vote.block > previous_slot)
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
			if let Some((value, block_height)) = election_access.check_consensus()?.has_consensus()
			{
				let (identifier, previous_value, previous_block_height) =
					election_access.properties()?;
				if previous_value != value && block_height > previous_block_height {
					election_access.delete();
					Hook::on_change(identifier.clone(), value);
					electoral_access
						.set_unsynchronised_state_map(identifier, Some(block_height))?;
				} else {
					log_or_panic!("Should be impossible to reach consensus with the same value and/or lower block_height");
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
			let mut counts: BTreeMap<Value, Vec<BlockHeight>> = BTreeMap::new();
			for vote in active_votes.clone() {
				counts
					.entry(vote.value)
					.and_modify(|slots| slots.push(vote.block))
					.or_insert(vec![vote.block]);
			}

			counts.iter().find_map(|(vote, blocks_height)| {
				let num_votes = blocks_height.len() as u32;
				if num_votes >= success_threshold {
					let mut blocks_height = blocks_height.clone();
					let (_, consensus_block_height, _) = {
						blocks_height
							.select_nth_unstable(threshold_from_share_count(num_votes) as usize)
					};
					Some((vote.clone(), *consensus_block_height))
				} else {
					None
				}
			})
		} else {
			None
		})
	}
}
