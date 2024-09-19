use crate::electoral_system::{
	ConsensusStatus, ConsensusVotes, ElectionIdentifierOf, ElectoralReadAccess, ElectoralSystem,
	ElectoralWriteAccess,
};
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

	#[allow(dead_code)]
	pub fn with_unsynchronised_settings(
		self,
		unsynchronised_settings: ES::ElectoralUnsynchronisedSettings,
	) -> Self {
		Self { unsynchronised_settings, ..self }
	}

	#[allow(dead_code)]
	pub fn with_electoral_settings(self, electoral_settings: ES::ElectoralSettings) -> Self {
		Self { electoral_settings, ..self }
	}

	#[allow(dead_code)]
	pub fn with_initial_election_state(
		self,
		extra: ES::ElectionIdentifierExtra,
		properties: ES::ElectionProperties,
		state: ES::ElectionState,
	) -> Self {
		Self { initial_election_state: Some((extra, properties, state)), ..self }
	}

	// Useful for testing check_consensus since we already have an election.
	pub fn build_with_initial_election(self) -> TestContext<ES> {
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
		assert_eq!(election.check_consensus(None, ConsensusVotes { votes: vec![] }).unwrap(), None);

		TestContext { setup, electoral_access }
	}

	// We may want to test initialisation of elections within on finalise, so *don't* want to
	// initialise an election in the utilities.
	pub fn build(self) -> TestContext<ES> {
		TestContext {
			setup: self.clone(),
			electoral_access: MockAccess::<ES>::new(
				self.unsynchronised_state,
				self.unsynchronised_settings,
				self.electoral_settings,
			),
		}
	}
}

impl<ES: ElectoralSystem> TestContext<ES> {
	/// Based on some authority count and votes, evaluate the consensus and the final state.
	#[allow(clippy::type_complexity)]
	#[track_caller]
	pub fn expect_consensus(
		self,
		consensus_votes: ConsensusVotes<ES>,
		expected_consensus: Option<ES::Consensus>,
	) -> Self {
		Self::expect_consensus_by(
			self,
			consensus_votes,
			expected_consensus,
			|new_consensus, expected_consensus| {
				assert_eq!(new_consensus, expected_consensus);
			},
		)
	}

	pub fn expect_consensus_by<C: FnOnce(Option<ES::Consensus>, Option<ES::Consensus>)>(
		self,
		mut consensus_votes: ConsensusVotes<ES>,
		expected_consensus: Option<ES::Consensus>,
		check: C,
	) -> Self {
		assert!(consensus_votes.num_authorities() > 0, "Cannot have zero authorities.");

		use rand::seq::SliceRandom;
		consensus_votes.votes.shuffle(&mut rand::thread_rng());

		// Expect only one election.
		let current_election_id = self.only_election_id();

		let new_consensus = self
			.electoral_access
			.election(current_election_id)
			.unwrap()
			.check_consensus(None, consensus_votes)
			.unwrap();

		// Should assert on some condition about the consensus.
		check(new_consensus.clone(), expected_consensus);

		self.inner_force_consensus_update(
			current_election_id,
			if let Some(consensus) = new_consensus {
				ConsensusStatus::Gained { most_recent: None, new: consensus }
			} else {
				ConsensusStatus::None
			},
		)
	}

	pub fn only_election_id(&self) -> ElectionIdentifierOf<ES> {
		self.all_election_ids()
			.into_iter()
			.exactly_one()
			.expect("Expected exactly one election.")
	}

	pub fn all_election_ids(&self) -> Vec<ElectionIdentifierOf<ES>> {
		self.electoral_access.election_identifiers()
	}

	/// Update the current consensus without processing any votes.
	pub fn force_consensus_update(self, new_consensus: ConsensusStatus<ES::Consensus>) -> Self {
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
		new_consensus: ConsensusStatus<ES::Consensus>,
	) -> Self {
		let mut electoral_access = self.electoral_access.clone();
		electoral_access
			.election_mut(election_id)
			.unwrap()
			.set_consensus_status(new_consensus);

		Self { electoral_access, ..self }
	}

	/// Test the finalization of the election.
	///
	/// `pre_finalize_checks` is a closure that is called with a read-only access to the electoral
	/// state before finalization.
	///
	/// `post_finalize_checks` is a list of checks that are run after finalization. These checks are
	///
	/// See [register_checks] and
	#[track_caller]
	pub fn test_on_finalize(
		mut self,
		on_finalize_context: &ES::OnFinalizeContext,
		pre_finalize_checks: impl FnOnce(&MockAccess<ES>),
		post_finalize_checks: impl IntoIterator<Item = Check<ES>>,
	) -> Self {
		let pre_finalize = self.electoral_access.clone();
		// TODO: Move 'hook' static local checks into MockAccess so we can remove this.
		pre_finalize_checks(&pre_finalize);

		self.electoral_access.finalize_elections(on_finalize_context).unwrap();

		let post_finalize = self.electoral_access.clone();
		for check in post_finalize_checks {
			check.check(&pre_finalize, &post_finalize);
		}
		self
	}
}

/// Allows registering checks for an electoral system. Once registered, the checks can be used
/// through the `Check` struct.
///
/// Example:
///
/// ```ignore
/// register_checks! {
///     MonotonicMedianTest {
///         monotonically_increasing_state(pre_finalize, post_finalize) {
///             assert!(
///                 post_finalize.unsynchronised_state().unwrap() >= pre_finalize.unsynchronised_state().unwrap(),
///                 "Expected state to increase post-finalization."
///             );
///         },
///         // ..
///     }
/// }
/// ```
///
///
/// Alternatively, you can specify extra constraints for the electoral system instead of using a
/// concrete type:
///
/// ```ignore
/// register_checks! {
///     #[extra_constraints: ES: ElectoralSystem, ES::ElectionIdentifierExtra: Default]#
///     monotonically_increasing_state(pre_finalize, post_finalize) {
///         assert!(
///             post_finalize.unsynchronised_state().unwrap() >= pre_finalize.unsynchronised_state().unwrap(),
///             "Expected state to increase post-finalization."
///         );
///     },
/// }
/// ```
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
			#[extra_constraints: $( $t:ty : $tc:path ),+]#
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
	last_election_deleted(pre_finalize, post_finalize) {
		let last_election_id = *pre_finalize.election_identifiers().last().expect("Expected an election before finalization.");
		assert!(
			post_finalize.election(last_election_id).is_err(),
			"Expected election {:?} to be deleted. Elections before: {:?}. After: {:?}",
			last_election_id,
			pre_finalize.election_identifiers(),
			post_finalize.election_identifiers(),
		);
	},
	election_id_incremented(pre_finalize, post_finalize) {
		assert_eq!(
			pre_finalize.next_umi().next_identifier().unwrap(),
			post_finalize.next_umi(),
			"Expected the election id to be incremented.",
		);
	},
	all_elections_deleted(pre_finalize, post_finalize) {
		assert!(
			!pre_finalize.election_identifiers().is_empty(),
			"Expected elections before finalization. This check makes no sense otherwise.",
		);
		assert!(
			post_finalize.election_identifiers().is_empty(),
			"Expected no elections after finalization.",
		);
	},
}

type CheckFn<ES> = Box<dyn Fn(&MockAccess<ES>, &MockAccess<ES>)>;

/// Checks that can be applied post-finalization.
pub struct Check<ES: ElectoralSystem> {
	check_fn: CheckFn<ES>,
}

impl<ES: ElectoralSystem> Check<ES> {
	pub fn new(check_fn: impl Fn(&MockAccess<ES>, &MockAccess<ES>) + 'static) -> Self {
		Self { check_fn: Box::new(check_fn) }
	}

	pub fn check(&self, pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>) {
		(self.check_fn)(pre_finalize, post_finalize)
	}
}
