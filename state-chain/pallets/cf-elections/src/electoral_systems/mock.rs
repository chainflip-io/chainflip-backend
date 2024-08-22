use crate::{
	electoral_system::{
		AuthorityVoteOf, ElectionReadAccess, ElectoralSystem, ElectoralWriteAccess,
		VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_primitives::AuthorityCount;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use std::{cell::RefCell, ops::Deref};

/// Simple wrapped data type for testing.
macro_rules! impl_wrapped_data {
	( $name:ident, $t:ty ) => {
		#[derive(
			Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, Default,
		)]
		pub struct $name($t);
		impl Deref for $name {
			type Target = $t;
			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}
	};
}

thread_local! {
	static VOTE_DESIRED: RefCell<bool> = RefCell::new(true);
	static VOTE_NEEDED: RefCell<bool> = RefCell::new(true);
	static VOTE_VALID: RefCell<bool> = RefCell::new(true);
	static CONSENSUS: RefCell<Option<Consensus>> = RefCell::new(None);
	static ON_FINALIZE_RETURN: RefCell<OnFinalizeReturn> = RefCell::new(Default::default());
}

impl_wrapped_data!(OnFinalizeReturn, u32);
impl_wrapped_data!(Consensus, u32);

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

	pub fn set_consensus(consensus: Option<Consensus>) {
		CONSENSUS.with(|v| *v.borrow_mut() = consensus);
	}

	pub fn consensus() -> Option<Consensus> {
		CONSENSUS.with(|v| *v.borrow())
	}

	pub fn set_on_finalize_return(on_finalize_return: OnFinalizeReturn) {
		ON_FINALIZE_RETURN.with(|v| *v.borrow_mut() = on_finalize_return);
	}

	pub fn on_finalize_return() -> OnFinalizeReturn {
		ON_FINALIZE_RETURN.with(|v| *v.borrow())
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
	type Consensus = Consensus;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = OnFinalizeReturn;

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<(), CorruptStorageError> {
		todo!()
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		_electoral_access: &mut ElectoralAccess,
		_election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		Ok(Self::on_finalize_return())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		_votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		_authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		Ok(Self::consensus())
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
