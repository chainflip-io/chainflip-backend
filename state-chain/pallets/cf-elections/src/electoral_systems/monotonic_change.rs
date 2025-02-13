use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralSystemTypes, ElectoralWriteAccess,
		PartialVoteOf, VoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError,
};
use cf_runtime_utilities::log_or_panic;
use cf_utilities::{success_threshold_from_share_count, threshold_from_share_count};
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
pub struct MonotonicChange<
	Identifier,
	Value,
	BlockHeight,
	Settings,
	Hook,
	ValidatorId,
	StateChainBlockNumber,
> {
	_phantom: core::marker::PhantomData<(
		Identifier,
		Value,
		BlockHeight,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	)>,
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
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	>
	MonotonicChange<Identifier, Value, BlockHeight, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	pub fn watch_for_change<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		identifier: Identifier,
		previous_value: Value,
	) -> Result<(), CorruptStorageError> {
		let previous_block_height =
			ElectoralAccess::unsynchronised_state_map(&identifier)?.unwrap_or_default();
		ElectoralAccess::new_election((), (identifier, previous_value, previous_block_height), ())?;
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
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystemTypes
	for MonotonicChange<
		Identifier,
		Value,
		BlockHeight,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	>
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = Identifier;
	type ElectoralUnsynchronisedStateMapValue = BlockHeight;
	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = (Identifier, Value, BlockHeight);
	type ElectionState = ();
	type VoteStorage = vote_storage::change::MonotonicChange<Value, BlockHeight>;
	type Consensus = (Value, BlockHeight);
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		BlockHeight: Member + Parameter + Eq + Ord + Copy + Default,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: OnChangeHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for MonotonicChange<
		Identifier,
		Value,
		BlockHeight,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	>
{
	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn is_vote_needed(
		(_, _, current_vote): (VotePropertiesOf<Self>, PartialVoteOf<Self>, AuthorityVoteOf<Self>),
		(_, proposed_vote): (PartialVoteOf<Self>, VoteOf<Self>),
	) -> bool {
		match current_vote {
			AuthorityVoteOf::<Self>::Vote(current_vote) =>
				current_vote.value != proposed_vote.value,
			// Could argue for either true or false. If the `PartialVote` is never reconstructed and
			// becomes invalid, then this function will be bypassed and the vote will be considered
			// needed. So false is safe, and true will likely result in unneeded voting.
			_ => false,
		}
	}

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let election_access = ElectoralAccess::election_mut(election_identifier);
			if let Some((value, block_height)) = election_access.check_consensus()?.has_consensus()
			{
				let (identifier, previous_value, previous_block_height) =
					election_access.properties()?;
				if previous_value != value && block_height > previous_block_height {
					election_access.delete();
					Hook::on_change(identifier.clone(), value);
					ElectoralAccess::set_unsynchronised_state_map(identifier, Some(block_height));
				} else {
					// We don't expect this to be hit, since we should have filtered out any votes
					// that would cause this in check_consensus.
					log_or_panic!(
						"No change detected for {:?}, election_identifier: {:?}",
						identifier,
						election_identifier
					);
				}
			}
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		let success_threshold = success_threshold_from_share_count(num_authorities);
		let (_, previous_value, previous_block) = election_access.properties()?;

		Ok(if num_active_votes >= success_threshold {
			let mut counts: BTreeMap<Value, Vec<BlockHeight>> = BTreeMap::new();
			for vote in active_votes.clone().into_iter().filter(|monotonic_change_vote| {
				monotonic_change_vote.block > previous_block &&
					previous_value != monotonic_change_vote.value
			}) {
				counts.entry(vote.value).or_default().push(vote.block);
			}

			counts.iter().find_map(|(vote, blocks_height)| {
				let num_votes = blocks_height.len() as u32;
				if num_votes >= success_threshold {
					let mut blocks_height = blocks_height.clone();
					let (_, consensus_block_height, _) = {
						blocks_height.select_nth_unstable(threshold_from_share_count(
							num_authorities,
						) as usize)
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
