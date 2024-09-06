use crate::{
	electoral_system::{
		ConsensusStatus, ElectionIdentifierOf, ElectoralReadAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::VoteStorage,
};
use cf_primitives::AuthorityCount;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};

pub mod access;

pub use access::*;
use itertools::Itertools;

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
pub struct TestSetup<ES: ElectoralSystem> {
	unsynchronised_state: ES::ElectoralUnsynchronisedState,
	unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
	electoral_settings: ES::ElectoralSettings,
	initial_election_state:
		Option<(ES::ElectionIdentifierExtra, ES::ElectionProperties, ES::ElectionState)>,
}

impl<ES: ElectoralSystem> Default for TestSetup<ES>
where
	ES::ElectoralUnsynchronisedState: Default,
	ES::ElectoralUnsynchronisedSettings: Default,
	ES::ElectoralSettings: Default,
{
	fn default() -> Self {
		Self {
			unsynchronised_state: Default::default(),
			unsynchronised_settings: Default::default(),
			electoral_settings: Default::default(),
			initial_election_state: None,
		}
	}
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
pub struct TestContext<ES: ElectoralSystem> {
	setup: TestSetup<ES>,
	electoral_access: MockAccess<ES>,
	previous_consensus: Option<ES::Consensus>,
}

impl<ES: ElectoralSystem> TestSetup<ES>
where
	ES::ElectionIdentifierExtra: Default,
	ES::ElectionProperties: Default,
	ES::ElectionState: Default,
{
	pub fn with_unsynchronised_state(
		self,
		unsynchronised_state: ES::ElectoralUnsynchronisedState,
	) -> Self {
		Self { unsynchronised_state, ..self }
	}

	pub fn with_unsynchronised_settings(
		self,
		unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
	) -> Self {
		Self { unsynchronised_settings, ..self }
	}

	pub fn with_electoral_settings(self, electoral_settings: ES::ElectoralSettings) -> Self {
		Self { electoral_settings, ..self }
	}

	pub fn with_initial_election_state(
		self,
		extra: ES::ElectionIdentifierExtra,
		properties: ES::ElectionProperties,
		state: ES::ElectionState,
	) -> Self {
		Self { initial_election_state: Some((extra, properties, state)), ..self }
	}

	pub fn build(self) -> TestContext<ES> {
		let setup = self.clone();
		let mut electoral_access = MockAccess::<ES>::new(
			self.unsynchronised_state,
			self.unsynchronised_settings,
			self.electoral_settings,
		);

		let (election_identifier_extra, election_properties, election_state) =
			self.initial_election_state.unwrap_or_default();

		let election = electoral_access
			.new_election(election_identifier_extra, election_properties, election_state)
			.unwrap();

		// A new election should not have consensus at any authority count.
		assert_eq!(election.check_consensus(None, vec![], 0).unwrap(), None);
		assert_eq!(election.check_consensus(None, vec![], 150).unwrap(), None);

		TestContext { setup, electoral_access, previous_consensus: None }
	}
}

impl<ES: ElectoralSystem> TestContext<ES> {
	/// Based on some authority count and votes, evaluate the consensus and the final state.
	#[track_caller]
	pub fn expect_consensus(
		self,
		authority_count: AuthorityCount,
		mut votes: Vec<(VotePropertiesOf<ES>, <ES::Vote as VoteStorage>::Vote)>,
		expected_consensus: Option<ES::Consensus>,
	) -> Self {
		assert!(
			authority_count >= votes.len() as AuthorityCount,
			"Cannot have more votes than authorities."
		);
		assert!(authority_count > 0, "Cannot have zero authorities.");

		use rand::seq::SliceRandom;
		votes.shuffle(&mut rand::thread_rng());

		// Expect only one election.
		let current_election_id = self.only_election_id();

		let consensus = self
			.electoral_access
			.election(current_election_id)
			.unwrap()
			.check_consensus(self.previous_consensus.as_ref(), votes, authority_count)
			.unwrap();

		assert_eq!(consensus, expected_consensus);

		self.inner_force_consensus_update(current_election_id, consensus)
	}

	pub fn only_election_id(&self) -> ElectionIdentifierOf<ES> {
		self.all_election_ids()
			.into_iter()
			.exactly_one()
			.expect("Expected exactly one election.")
	}

	pub fn latest_election_id(&self) -> ElectionIdentifierOf<ES> {
		*self.all_election_ids().last().expect("Expected at least one election.")
	}

	pub fn all_election_ids(&self) -> Vec<ElectionIdentifierOf<ES>> {
		self.electoral_access.election_identifiers()
	}

	pub fn force_consensus_update(self, new_consensus: Option<ES::Consensus>) -> Self {
		let id = self.only_election_id();
		self.inner_force_consensus_update(id, new_consensus)
	}

	pub fn access(&self) -> &MockAccess<ES> {
		&self.electoral_access
	}

	#[track_caller]
	fn inner_force_consensus_update(
		self,
		election_id: ElectionIdentifierOf<ES>,
		new_consensus: Option<ES::Consensus>,
	) -> Self {
		let mut electoral_access = self.electoral_access.clone();
		electoral_access.election_mut(election_id).unwrap().set_consensus_status(
			match (self.previous_consensus, new_consensus.clone()) {
				(Some(previous), Some(new)) if previous != new =>
					ConsensusStatus::Changed { previous, new },
				(Some(_), Some(current)) => ConsensusStatus::Unchanged { current },
				(None, Some(new)) => ConsensusStatus::Gained { most_recent: None, new },
				(Some(previous), None) => ConsensusStatus::Lost { previous },
				(None, None) => ConsensusStatus::None,
			},
		);

		Self { previous_consensus: new_consensus, electoral_access, ..self }
	}

	#[track_caller]
	pub fn test_on_finalize(
		mut self,
		on_finalize_context: &ES::OnFinalizeContext,
		pre_finalize_checks: impl FnOnce(&MockAccess<ES>),
		post_finalize_checks: impl IntoIterator<Item = Box<dyn ElectoralSystemCheck<ES>>>,
	) -> Self {
		let pre_finalize = self.electoral_access.clone();
		pre_finalize_checks(&pre_finalize);
		self.electoral_access.finalize_elections(on_finalize_context).unwrap();
		let post_finalize = self.electoral_access.clone();
		for check in post_finalize_checks {
			check.check(&pre_finalize, &post_finalize);
		}
		self
	}
}

pub trait ElectoralSystemCheck<ES: ElectoralSystem> {
	#[track_caller]
	fn check(&self, pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>);
}

impl<ES: ElectoralSystem> ElectoralSystemCheck<ES> for () {
	fn check(&self, _pre_finalize: &MockAccess<ES>, _post_finalize: &MockAccess<ES>) {}
}

impl<ES: ElectoralSystem, A: ElectoralSystemCheck<ES>, B: ElectoralSystemCheck<ES>>
	ElectoralSystemCheck<ES> for (A, B)
{
	fn check(&self, pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>) {
		self.0.check(&pre_finalize, &post_finalize);
		self.1.check(&pre_finalize, &post_finalize);
	}
}

#[macro_export]
macro_rules! register_checks {
	(
		$system:ident {
			$(
				$check_name:ident($arg_1:ident, $arg_2:ident) $check_body:block
			),+ $(,)*
		}
	) => {
		impl Check<$system>{
			$(
				pub fn $check_name() -> Self {
					Self::new(#[track_caller] |$arg_1, $arg_2| $check_body)
				}
			)+
		}
	};
	(
		$(
			#[ extra_constraints: $( $t:ty : $tc:path ),+ ]#
		)?
		$(
			$check_name:ident($arg_1:ident, $arg_2:ident) $check_body:block
		),+ $(,)*
	) => {
		impl<ES: ElectoralSystem> Check<ES>
			$( where $( $t: $tc ),+ )?
		{
			$(
				pub fn $check_name() -> Self {
					Self::new(#[track_caller] |$arg_1, $arg_2| $check_body)
				}
			)+
		}
	};
}

// Simple examples with register_check:
register_checks! {
	assert_unchanged(pre_finalize, post_finalize) {
		assert_eq!(pre_finalize, post_finalize);
	},
	assert_changed(pre_finalize, post_finalize) {
		assert_ne!(pre_finalize, post_finalize);
	},
	unsynchronised_state_is_updated(pre_finalize, post_finalize) {
		assert_eq!(
			post_finalize.unsynchronised_state().unwrap(),
			pre_finalize.unsynchronised_state().unwrap(),
		);
	},
	unsynchronised_state_is_not_updated(pre_finalize, post_finalize) {
		assert_eq!(
			post_finalize.unsynchronised_state().unwrap(),
			pre_finalize.unsynchronised_state().unwrap()
		);
	},
	last_election_deleted(pre_finalize, post_finalize) {
		let last_election_id = *pre_finalize.election_identifiers().last().expect("Expected an election before finalization.");
		assert!(post_finalize.election(last_election_id).is_err(), "Expected election to be deleted.");
	},
}

#[macro_export]
macro_rules! boxed_check {
	($check:expr) => {
		Box::new($check) as Box<dyn $crate::electoral_systems::mocks::ElectoralSystemCheck<_>>
	};
}

#[macro_export]
macro_rules! checks {
	( $($check:expr),+ $(,)?) => {
		vec! [
			$(
				$crate::boxed_check!($check),
			)+
		]
	};
}

pub struct Check<ES: ElectoralSystem> {
	check_fn: Box<dyn Fn(&MockAccess<ES>, &MockAccess<ES>)>,
}

impl<ES: ElectoralSystem> Check<ES> {
	pub fn new(check_fn: impl Fn(&MockAccess<ES>, &MockAccess<ES>) + 'static) -> Self {
		Self { check_fn: Box::new(check_fn) }
	}
}

impl<ES: ElectoralSystem> ElectoralSystemCheck<ES> for Check<ES> {
	fn check(&self, pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>) {
		(self.check_fn)(pre_finalize, post_finalize)
	}
}
