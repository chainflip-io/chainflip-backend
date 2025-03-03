use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionIdentifierOf, ElectionReadAccess,
		ElectionWriteAccess, ElectoralSystem, ElectoralSystemTypes, ElectoralWriteAccess,
		PartialVoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError,
};
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// This electoral system detects if something occurred or not. Voters simply vote if something
/// happened, and if they haven't seen it happen, they don't vote.
<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
pub struct ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber> {
========
pub struct WitnessSomethingByIdentifier<
	Identifier,
	Value,
	Settings,
	Hook,
	ValidatorId,
	StateChainBlockNumber,
> {
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
	_phantom: core::marker::PhantomData<(
		Identifier,
		Value,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	)>,
}

<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
pub trait ExactValueHook<Identifier, Value> {
	fn on_consensus(id: Identifier, value: Value);
========
pub trait WitnessSomethingHook<Identifier, Value> {
	fn on_successful_witness(id: Identifier, value: Value);
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
	fn should_expire_election(id: Identifier) -> bool;
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	pub fn witness_exact_value<
========
		Hook: WitnessSomethingHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	>
	WitnessSomethingByIdentifier<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
{
	pub fn witness_something<
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
		ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static,
	>(
		identifier: Identifier,
	) -> Result<(), CorruptStorageError> {
		ElectoralAccess::new_election((), identifier, ())?;
		Ok(())
	}
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystemTypes
	for ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
========
		Hook: WitnessSomethingHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystemTypes
	for WitnessSomethingByIdentifier<
		Identifier,
		Value,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	>
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = Identifier;
	type ElectionState = ();
	type VoteStorage = vote_storage::bitmap::Bitmap<Value>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();
}

impl<
		Identifier: Member + Parameter + Ord,
		Value: Member + Parameter + Eq + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
		Hook: ExactValueHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for ExactValue<Identifier, Value, Settings, Hook, ValidatorId, StateChainBlockNumber>
========
		Hook: WitnessSomethingHook<Identifier, Value> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
		StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for WitnessSomethingByIdentifier<
		Identifier,
		Value,
		Settings,
		Hook,
		ValidatorId,
		StateChainBlockNumber,
	>
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
{
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_needed(
		(_, current_partial_vote, _): (
			VotePropertiesOf<Self>,
			PartialVoteOf<Self>,
			AuthorityVoteOf<Self>,
		),
		(proposed_partial_vote, _): (PartialVoteOf<Self>, crate::VoteOf<Self>),
	) -> bool {
		current_partial_vote != proposed_partial_vote
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let election_access = ElectoralAccess::election_mut(election_identifier);
<<<<<<<< HEAD:state-chain/pallets/cf-elections/src/electoral_systems/exact_value.rs
			let identifier = election_access.properties()?;
			if let Some(witnessed_value) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();
				Hook::on_consensus(identifier, witnessed_value);
			} else if Hook::should_expire_election(identifier) {
========
			if let Some(witness_data) = election_access.check_consensus()?.has_consensus() {
				let identifier = election_access.properties()?;
				election_access.delete();
				Hook::on_successful_witness(identifier, witness_data);
			} else if Hook::should_expire_election(election_access.properties()?) {
>>>>>>>> 92750eb97 (chore: rename Egress witnessing -> witnessing something):state-chain/pallets/cf-elections/src/electoral_systems/witness_something_by_identifier.rs
				election_access.delete();
			}
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
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
