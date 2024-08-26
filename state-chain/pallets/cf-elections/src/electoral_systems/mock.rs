use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusStatus, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier, UniqueMonotonicIdentifier,
};
use cf_primitives::AuthorityCount;
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeMap};

thread_local! {
	static VOTE_DESIRED: RefCell<bool> = RefCell::new(true);
	static VOTE_NEEDED: RefCell<bool> = RefCell::new(true);
	static VOTE_VALID: RefCell<bool> = RefCell::new(true);
	static ASSUME_CONSENSUS: RefCell<bool> = RefCell::new(false);
	static CONSENSUS_STATUS: RefCell<
		BTreeMap<UniqueMonotonicIdentifier, ConsensusStatus<AuthorityCount>>
	> = RefCell::new(Default::default());
}

/// Mock electoral system for testing.
///
/// It behaves as follows:
/// - `vote_desired`, `vote_needed`, and `vote_valid` are all set to `true` by default.
/// - `assume_consensus` is set to `false` by default.
/// - `consensus_status` is set to `None` by default.
///
/// If assume_consensus is set to `true`, then the consensus value will be the number of votes.
pub struct MockElectoralSystem;

impl MockElectoralSystem {
	pub fn set_vote_desired(desired: bool) {
		VOTE_DESIRED.with(|v| *v.borrow_mut() = desired);
	}

	pub fn set_vote_needed(needed: bool) {
		VOTE_NEEDED.with(|v| *v.borrow_mut() = needed);
	}

	pub fn set_vote_valid(valid: bool) {
		VOTE_VALID.with(|v| *v.borrow_mut() = valid);
	}

	pub fn vote_desired() -> bool {
		VOTE_DESIRED.with(|v| *v.borrow())
	}

	pub fn vote_needed() -> bool {
		VOTE_NEEDED.with(|v| *v.borrow())
	}

	pub fn vote_valid() -> bool {
		VOTE_VALID.with(|v| *v.borrow())
	}

	pub fn set_assume_consensus(assume: bool) {
		ASSUME_CONSENSUS.with(|v| *v.borrow_mut() = assume);
	}

	pub fn should_assume_consensus() -> bool {
		ASSUME_CONSENSUS.with(|v| *v.borrow())
	}

	pub fn consensus_status(umi: UniqueMonotonicIdentifier) -> ConsensusStatus<AuthorityCount> {
		CONSENSUS_STATUS.with_borrow(|v| v.get(&umi).cloned().unwrap_or(ConsensusStatus::None))
	}

	pub fn set_consensus_status(
		umi: UniqueMonotonicIdentifier,
		consensus_status: ConsensusStatus<AuthorityCount>,
	) {
		CONSENSUS_STATUS.with_borrow_mut(|v| {
			v.insert(umi, consensus_status);
		});
	}

	pub fn reset() {
		Self::set_vote_desired(true);
		Self::set_vote_needed(true);
		Self::set_vote_valid(true);
		Self::set_assume_consensus(false);
	}

	pub fn delete_elections<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<()>>,
	) -> Result<(), CorruptStorageError> {
		for id in election_identifiers {
			electoral_access.election_mut(id)?.delete();
		}

		Ok(())
	}

	pub fn delete_all_elections<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<()>>,
	) -> Result<(), CorruptStorageError> {
		Self::delete_elections(electoral_access, election_identifiers)
	}
}

impl ElectoralSystem for MockElectoralSystem {
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	// TODO: mock the vote storage
	type Vote =
		vote_storage::individual::Individual<(), vote_storage::individual::shared::Shared<()>>;
	type Consensus = AuthorityCount;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<(), CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for id in election_identifiers {
			// Read the current consensus status and save it.
			Self::set_consensus_status(
				*id.unique_monotonic(),
				electoral_access.election_mut(id)?.check_consensus()?,
			);
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		_authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		Ok(if Self::should_assume_consensus() { Some(votes.len() as AuthorityCount) } else { None })
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier_with_extra: crate::electoral_system::ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(Self::vote_desired())
	}

	fn is_vote_needed(
		_current_vote: (
			VotePropertiesOf<Self>,
			<Self::Vote as VoteStorage>::PartialVote,
			AuthorityVoteOf<Self>,
		),
		_proposed_vote: (
			<Self::Vote as VoteStorage>::PartialVote,
			<Self::Vote as VoteStorage>::Vote,
		),
	) -> bool {
		Self::vote_needed()
	}

	fn is_vote_valid<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: crate::electoral_system::ElectionIdentifierOf<Self>,
		_election_access: &ElectionAccess,
		_partial_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<bool, CorruptStorageError> {
		Ok(Self::vote_valid())
	}
}
