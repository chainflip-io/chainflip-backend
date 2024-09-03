use crate::{
	electoral_system::{
		AuthorityVoteOf, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_primitives::AuthorityCount;
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::Itertools;
use sp_std::vec::Vec;

pub trait MedianChangeHook<Value> {
	fn on_change(value: Value);
}

/// This electoral system is for tracking a monotonically increasing `Value` that authorities may
/// not have the same view of, i.e. they may see slightly different values. It calculates a median
/// of all the authority votes and stores the latest median in the `ElectoralUnsynchronisedState`,
/// but only if the new median is larger than the last. Each time consensus is gained, everyone is
/// asked to revote. *IMPORTANT*: This method requires atleast 2/3 to artifically increase the
/// median, 1/3 to "reliably" stop it from increasing (Note a smaller number of validators may be
/// able to stop it from increasing some of the time, but not consistently and importantly the
/// overall increase rate would be unaffected), and the `Value` cannot be decreased.
///
/// `Settings` can be used by governance to provide information to authorities about exactly how
/// they should `vote`.
pub struct MonotonicMedian<Value, Settings, Hook> {
	_phantom: core::marker::PhantomData<(Value, Settings, Hook)>,
}
impl<
		Value: MaybeSerializeDeserialize + Member + Parameter + Ord,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: MedianChangeHook<Value> + 'static,
	> ElectoralSystem for MonotonicMedian<Value, Settings, Hook>
{
	type ElectoralUnsynchronisedState = Value;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	type Vote =
		vote_storage::individual::Individual<(), vote_storage::individual::shared::Shared<Value>>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = Value;

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		_context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if let Some(consensus) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();
				electoral_access.new_election((), (), ())?;
				electoral_access.mutate_unsynchronised_state(
					|_electoral_access, unsynchronised_state| {
						if consensus > *unsynchronised_state {
							*unsynchronised_state = consensus.clone();
							Hook::on_change(consensus);
						}

						Ok(())
					},
				)?;
			}
		} else {
			electoral_access.new_election((), (), ())?;
		}

		electoral_access.unsynchronised_state()
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		mut votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let votes_count = votes.len();
		let threshold = success_threshold_from_share_count(authorities) as usize;
		Ok(if votes_count != 0 && votes_count >= threshold {
			// Calculating the median this way means atleast 2/3 of validators would be needed to
			// increase the calculated median.
			let (_, (_properties, median_vote), _) =
				votes.select_nth_unstable(authorities as usize - threshold);
			Some(median_vote.clone())
		} else {
			None
		})
	}
}

#[cfg(test)]
mod test_monotonic_median {
	use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};

	use super::*;
	use crate::electoral_system::{
		mocks::MockAccess, ConsensusStatus, ElectionIdentifierOf, ElectoralReadAccess,
	};

	pub trait ElectoralSystemCheck<ES: ElectoralSystem> {
		#[track_caller]
		fn check(pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>);
	}

	impl<ES: ElectoralSystem> ElectoralSystemCheck<ES> for () {
		fn check(_pre_finalize: &MockAccess<ES>, _post_finalize: &MockAccess<ES>) {}
	}

	impl<ES: ElectoralSystem, A: ElectoralSystemCheck<ES>, B: ElectoralSystemCheck<ES>>
		ElectoralSystemCheck<ES> for (A, B)
	{
		fn check(pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>) {
			A::check(&pre_finalize, &post_finalize);
			B::check(&pre_finalize, &post_finalize);
		}
	}

	macro_rules! define_checks {
		(
			$(
				$check_name:ident $(
					#[ extra_constraints: $( $t:ty : $tc:path ),+ ]#
				)? =>
				_($arg_1:ident, $arg_2:ident) $check_body:block
			),+ $(,)*
		) => {
			$(
				#[derive(Default)]
				pub struct $check_name<ES>(core::marker::PhantomData<ES>);
				impl<ES: ElectoralSystem> ElectoralSystemCheck<ES> for $check_name<ES>
					$( where $( $t: $tc ),+ )?
				{
					#[track_caller]
					fn check(pre_finalize: &MockAccess<ES>, post_finalize: &MockAccess<ES>) {
						let ($arg_1, $arg_2) = (pre_finalize, post_finalize);
						$check_body
					}
				}
			)+
		};
	}

	// Example:
	define_checks! {
		AssertUnchanged => _(pre_finalize, post_finalize) {
			assert_eq!(pre_finalize, post_finalize);
		},
	}

	macro_rules! compose_checks_for {
		// --- inner macro methods ---
		( @inner($mock_es:ty) $last:ident $(,)? ) => {
			( $last<$mock_es>, () )
		};
		( @inner($mock_es:ty) $fst:ident, $($rest:tt)+ ) => {
			( $fst<$mock_es>, compose_checks_for! { @inner($mock_es) $($rest)+ } )
		};
		// --- entry point ---
		( $mock_es:ty, $($rest:tt)+ $(,)? ) => {
			compose_checks_for! { @inner($mock_es) $($rest)+ }
		};
	}

	pub struct MockHook;

	impl<T> MedianChangeHook<T> for MockHook {
		fn on_change(_value: T) {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(true));
		}
	}

	impl MockHook {
		pub fn has_been_called() -> bool {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.get())
		}

		pub fn reset() {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(false));
		}
	}

	thread_local! {
		pub static HOOK_HAS_BEEN_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
	struct TestSetup<ES: ElectoralSystem> {
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

			// Reset the hook state every time we run a new test.
			MockHook::reset();

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
		pub fn on_finalize_checks<PostChecks: ElectoralSystemCheck<ES>>(
			mut self,
			on_finalize_context: &ES::OnFinalizeContext,
			pre_finalize_checks: impl FnOnce(&MockAccess<ES>),
			additional_post_finalize_checks: impl FnOnce(&MockAccess<ES>, &MockAccess<ES>),
		) -> Self {
			let pre_finalize = self.electoral_access.clone();
			pre_finalize_checks(&pre_finalize);
			self.electoral_access.finalize_elections(on_finalize_context).unwrap();
			let post_finalize = self.electoral_access.clone();
			PostChecks::check(&pre_finalize, &post_finalize);
			additional_post_finalize_checks(&pre_finalize, &post_finalize);
			self
		}
	}

	define_checks! {
		MonotonicallyIncreasingState #[
			extra_constraints: <ES as ElectoralSystem>::ElectoralUnsynchronisedState: Ord
		]# => _(pre_finalize, post_finalize) {
			assert!(post_finalize.unsynchronised_state().unwrap() >= pre_finalize.unsynchronised_state().unwrap(),
				"Unsynchronised state can not decrease!");
		},
		HookHasBeenCalled => _(_pre, _post) {
			assert!(MockHook::has_been_called(), "Hook should have been called!");
		},
		HookNotBeenCalled => _(_pre, _post) {
			assert!(
				!MockHook::has_been_called(),
				"Hook should not have been called!"
			);
		},
	}

	type Invariants = compose_checks_for! {
		MonotonicMedian<u64, (), MockHook>,
		MonotonicallyIncreasingState
	};

	fn with_default_setup() -> TestSetup<MonotonicMedian<u64, (), MockHook>> {
		TestSetup::<MonotonicMedian<u64, (), MockHook>>::default()
	}

	fn with_default_context() -> TestContext<MonotonicMedian<u64, (), MockHook>> {
		TestSetup::<MonotonicMedian<u64, (), MockHook>>::default().build()
	}

	// --- TESTS ---

	#[test]
	fn check_consensus_correctly_calculates_median_when_all_authorities_vote() {
		const AUTHORITIES: AuthorityCount = 10;
		with_default_context().expect_consensus(
			AUTHORITIES,
			(0..AUTHORITIES).map(|v| ((), v as u64)).collect::<Vec<_>>(),
			Some(3), // lower tercile
		);
	}

	#[test]
	fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
		const AUTHORITY_COUNT: AuthorityCount = 10;
		let vote_count = cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT);
		let votes = (0..vote_count).map(|v| ((), v as u64)).collect::<Vec<_>>();

		with_default_context().expect_consensus(AUTHORITY_COUNT, votes, Some(3));
	}

	#[test]
	fn to_few_votes_consensus_not_possible() {
		const AUTHORITY_COUNT: AuthorityCount = 10;
		let vote_count = cf_utilities::success_threshold_from_share_count(AUTHORITY_COUNT) - 1;
		let votes = (0..vote_count).map(|v| ((), v as u64)).collect::<Vec<_>>();

		with_default_context().expect_consensus(AUTHORITY_COUNT, votes, None);
	}

	#[test]
	fn finalize_election_with_incremented_state() {
		let test @ TestContext { setup: TestSetup { unsynchronised_state, .. }, .. } =
			with_default_context();
		let new_unsynchronised_state = unsynchronised_state + 1;

		test.force_consensus_update(Some(new_unsynchronised_state))
			.on_finalize_checks::<(MonotonicallyIncreasingState<_>, HookHasBeenCalled<_>)>(
				&(),
				|_| {
					assert!(!MockHook::has_been_called(), "Hook should not have been called!");
				},
				|pre, post| {
					assert_eq!(pre.unsynchronised_state().unwrap(), unsynchronised_state);
					assert_eq!(post.unsynchronised_state().unwrap(), new_unsynchronised_state);
				},
			);
	}

	#[test]
	fn finalize_election_state_can_not_decrease() {
		const INTITIAL_STATE: u64 = 2;

		#[track_caller]
		fn assert_no_update(new_state: u64) {
			assert!(
				new_state <= INTITIAL_STATE,
				"This test is not valid if the new state is higher than the old."
			);
			with_default_setup()
				.with_unsynchronised_state(INTITIAL_STATE)
				.build()
				// It's possible for authorities to come to consensus on a lower state,
				// but this should not change the unsynchronised state.
				.force_consensus_update(Some(new_state))
				.on_finalize_checks::<(MonotonicallyIncreasingState<_>, HookNotBeenCalled<_>)>(
					&(),
					|_| {
						assert!(
							!MockHook::has_been_called(),
							"Hook should not have been called before finalization!"
						);
					},
					|pre, post| {
						assert_eq!(pre.unsynchronised_state().unwrap(), INTITIAL_STATE);
						assert_eq!(post.unsynchronised_state().unwrap(), INTITIAL_STATE);
					},
				);
		}

		// Lower state than the initial state should be invalid.
		assert_no_update(INTITIAL_STATE - 1);
		// Equal state to the initial state should be invalid.
		assert_no_update(INTITIAL_STATE);
	}

	#[test]
	fn minority_can_not_influence_consensus() {
		// Two ways of thinking about this:
		// - A superminority can prevent consensus value from advancing.
		// - A supermajority is required to advance the consensus value.
		//
		// This is why use the lower 33rd percentile vote. If we used the median, a simple majority
		// could influence the consensus value.

		const HONEST_VALUE: u64 = 5;
		const DISHONEST_VALUE: u64 = 10;
		const AUTHORITY_COUNT: u32 = 10;

		let threshold = cf_utilities::threshold_from_share_count(AUTHORITY_COUNT);
		let dishonest_votes = (0..threshold).map(|_| ((), DISHONEST_VALUE));
		let consent_votes = (0..(AUTHORITY_COUNT - threshold)).map(|_| ((), HONEST_VALUE));
		let all_votes = dishonest_votes.chain(consent_votes).collect::<Vec<_>>();

		with_default_context().expect_consensus(AUTHORITY_COUNT, all_votes, Some(HONEST_VALUE));
	}
}
