use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusStatus, ConsensusVotes, ElectionIdentifierOf,
		ElectoralSystemTypes, PartialVoteOf, VoteOf, VotePropertiesOf,
	},
	electoral_system_runner::{ElectoralSystemRunner, RunnerStorageAccessTrait},
	mock::Test,
	vote_storage, CorruptStorageError, RunnerStorageAccess, UniqueMonotonicIdentifier,
};
use cf_primitives::AuthorityCount;
use cf_traits::Chainflip;
use frame_support::instances::Instance1;
use sp_std::vec::Vec;
use std::{cell::RefCell, collections::BTreeMap};

// TODO: Consider using frame_support::parameter_types! with storage instead of using thread local.
thread_local! {
	static VOTE_DESIRED: RefCell<bool> = RefCell::new(true);
	static VOTE_NEEDED: RefCell<bool> = RefCell::new(true);
	static VOTE_VALID: RefCell<bool> = RefCell::new(true);
	static ASSUME_CONSENSUS: RefCell<bool> = RefCell::new(false);
	static CONSENSUS_STATUS: RefCell<
		BTreeMap<UniqueMonotonicIdentifier, ConsensusStatus<AuthorityCount>>
	> = RefCell::new(Default::default());
	static DELETE_ELECTIONS_ON_FINALIZE_CONSENSUS: RefCell<bool> = RefCell::new(false);
}

/// Mock electoral system for testing.
///
/// It behaves as follows:
/// - `vote_desired`, `vote_needed`, and `vote_valid` are all set to `true` by default.
/// - `assume_consensus` is set to `false` by default.
/// - `consensus_status` is set to `None` by default.
///
/// If assume_consensus is set to `true`, then the consensus value will be the number of votes.
pub struct MockElectoralSystemRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviourUpdate {
	VoteDesired(bool),
	VoteNeeded(bool),
	VoteValid(bool),
	AssumeConsensus(bool),
	DeleteOnFinalizeConsensus(bool),
}

impl BehaviourUpdate {
	pub fn apply(&self) {
		match self {
			BehaviourUpdate::VoteDesired(desired) => {
				VOTE_DESIRED.with(|v| *v.borrow_mut() = *desired);
			},
			BehaviourUpdate::VoteNeeded(needed) => {
				VOTE_NEEDED.with(|v| *v.borrow_mut() = *needed);
			},
			BehaviourUpdate::VoteValid(valid) => {
				VOTE_VALID.with(|v| *v.borrow_mut() = *valid);
			},
			BehaviourUpdate::AssumeConsensus(assume) => {
				ASSUME_CONSENSUS.with(|v| *v.borrow_mut() = *assume);
			},
			BehaviourUpdate::DeleteOnFinalizeConsensus(delete) => {
				DELETE_ELECTIONS_ON_FINALIZE_CONSENSUS.with(|v| *v.borrow_mut() = *delete);
			},
		}
	}
}

impl MockElectoralSystemRunner {
	pub fn vote_desired() -> bool {
		VOTE_DESIRED.with(|v| *v.borrow())
	}

	pub fn vote_needed() -> bool {
		VOTE_NEEDED.with(|v| *v.borrow())
	}

	pub fn vote_valid() -> bool {
		VOTE_VALID.with(|v| *v.borrow())
	}

	pub fn should_assume_consensus() -> bool {
		ASSUME_CONSENSUS.with(|v| *v.borrow())
	}

	pub fn should_delete_on_finalize_consensus() -> bool {
		DELETE_ELECTIONS_ON_FINALIZE_CONSENSUS.with(|v| *v.borrow())
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

	pub fn update(updates: &[BehaviourUpdate]) {
		updates.iter().for_each(BehaviourUpdate::apply);
	}

	pub fn reset() {
		Self::update(&[
			BehaviourUpdate::VoteDesired(true),
			BehaviourUpdate::VoteNeeded(true),
			BehaviourUpdate::VoteValid(true),
			BehaviourUpdate::AssumeConsensus(false),
			BehaviourUpdate::DeleteOnFinalizeConsensus(false),
		]);
		CONSENSUS_STATUS.with(|v| v.borrow_mut().clear());
	}
}

impl ElectoralSystemTypes for MockElectoralSystemRunner {
	type ValidatorId = <Test as Chainflip>::ValidatorId;
	type StateChainBlockNumber = u64;
	type ElectoralUnsynchronisedState = ();
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	// TODO: mock the vote storage
	type VoteStorage =
		vote_storage::individual::Individual<(), vote_storage::individual::shared::Shared<()>>;
	type Consensus = AuthorityCount;

	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();
}

impl ElectoralSystemRunner for MockElectoralSystemRunner {
	fn generate_vote_properties(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<(), CorruptStorageError> {
		Ok(())
	}

	fn on_finalize(
		election_identifiers: Vec<ElectionIdentifierOf<Self>>,
	) -> Result<(), CorruptStorageError> {
		for id in election_identifiers {
			let consensus =
				RunnerStorageAccess::<Test, Instance1>::check_election_consensus(id).unwrap();
			Self::set_consensus_status(*id.unique_monotonic(), consensus.clone());
			if consensus.has_consensus().is_some() && Self::should_delete_on_finalize_consensus() {
				RunnerStorageAccess::<Test, Instance1>::delete_election(id);
			}
		}

		Ok(())
	}

	fn check_consensus(
		_election_identifier: ElectionIdentifierOf<Self>,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		Ok(if Self::should_assume_consensus() {
			Some(consensus_votes.active_votes().len() as AuthorityCount)
		} else {
			None
		})
	}

	fn is_vote_desired(
		_election_identifier: ElectionIdentifierOf<Self>,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_current_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		Ok(Self::vote_desired())
	}

	fn is_vote_needed(
		_current_vote: (VotePropertiesOf<Self>, PartialVoteOf<Self>, AuthorityVoteOf<Self>),
		_proposed_vote: (PartialVoteOf<Self>, VoteOf<Self>),
	) -> bool {
		Self::vote_needed()
	}
}
