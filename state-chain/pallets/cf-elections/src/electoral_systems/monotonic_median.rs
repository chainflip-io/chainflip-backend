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
	use super::*;
	use crate::electoral_system::{
		mocks::MockElectoralSystem, ConsensusStatus, ElectoralReadAccess,
	};

	pub struct MockHook;

	impl MedianChangeHook<u64> for MockHook {
		fn on_change(_value: u64) {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.set(true));
		}
	}

	impl MockHook {
		pub fn get_hook_state() -> bool {
			HOOK_HAS_BEEN_CALLED.with(|hook_called| hook_called.get())
		}
	}

	thread_local! {
		pub static HOOK_HAS_BEEN_CALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
	}

	#[test]
	fn check_consensus_correctly_calculates_median_when_all_authorities_vote() {
		let mut electoral_system =
			MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(0, (), ());
		let mut votes = vec![((), 1), ((), 2), ((), 3), ((), 5), ((), 6), ((), 8), ((), 9)];
		let number_of_authorities = votes.len() as u32;

		use rand::{seq::SliceRandom, thread_rng};

		votes.shuffle(&mut thread_rng());

		let consensus = electoral_system
			.new_election((), (), ())
			.unwrap()
			.check_consensus(None, votes, number_of_authorities)
			.unwrap();

		assert_eq!(consensus, Some(3));
	}

	#[test]
	fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
		let mut electoral_system =
			MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(0, (), ());
		let mut votes = vec![((), 1), ((), 2), ((), 3), ((), 5), ((), 6), ((), 8), ((), 9)];
		let number_of_authorities = votes.len() as u32;

		use rand::{seq::SliceRandom, thread_rng};

		votes.shuffle(&mut thread_rng());

		let consensus = electoral_system
			.new_election((), (), ())
			.unwrap()
			.check_consensus(None, votes, number_of_authorities + (number_of_authorities / 2))
			.unwrap();

		assert_eq!(consensus, Some(5));
	}

	#[test]
	fn to_few_votes_consensus_not_possible() {
		let mut electoral_system =
			MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(0, (), ());
		let votes = vec![((), 1), ((), 2), ((), 3), ((), 5), ((), 6), ((), 8)];
		let number_of_authorities = votes.len() as u32;

		let consensus = electoral_system
			.new_election((), (), ())
			.unwrap()
			.check_consensus(None, votes, number_of_authorities * 2)
			.unwrap();

		assert_eq!(consensus, None);
	}

	#[test]
	fn no_votes_consensus_not_possible() {
		assert_eq!(
			MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(0, (), ())
				.new_election((), (), ())
				.unwrap()
				.check_consensus(None, vec![], 10)
				.unwrap(),
			None
		);
	}

	#[test]
	fn finalize_election() {
		const INIT_UNSYNCHRONISED_STATE: u64 = 1;
		const NEXT_UNSYNCHRONISED_STATE: u64 = 2;
		let mut electoral_system = MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(
			INIT_UNSYNCHRONISED_STATE,
			(),
			(),
		);
		let mut election = electoral_system.new_election((), (), ()).unwrap();
		election.set_consensus_status(ConsensusStatus::Changed {
			previous: INIT_UNSYNCHRONISED_STATE,
			new: NEXT_UNSYNCHRONISED_STATE,
		});
		electoral_system.finalize_elections(&()).unwrap();
		// Hock has been called
		assert!(MockHook::get_hook_state(), "Hook should have been called!");
		// Unsynchronised state has been updated
		assert_eq!(electoral_system.unsynchronised_state().unwrap(), NEXT_UNSYNCHRONISED_STATE);
	}

	#[test]
	fn finalize_election_state_can_not_decrease() {
		const INIT_UNSYNCHRONISED_STATE: u64 = 2;
		const NEXT_UNSYNCHRONISED_STATE: u64 = 1;
		let mut electoral_system = MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(
			INIT_UNSYNCHRONISED_STATE,
			(),
			(),
		);
		let mut election = electoral_system.new_election((), (), ()).unwrap();
		election.set_consensus_status(ConsensusStatus::Changed {
			previous: INIT_UNSYNCHRONISED_STATE,
			new: NEXT_UNSYNCHRONISED_STATE,
		});
		electoral_system.finalize_elections(&()).unwrap();
		// Hock has not been called
		assert!(
			MockHook::get_hook_state(),
			"Hook should not have been called if the consensus didn't change!"
		);
		// Unsynchronised state has not been updated
		assert_eq!(electoral_system.unsynchronised_state().unwrap(), INIT_UNSYNCHRONISED_STATE);
	}

	#[test]
	fn minority_can_not_influence_consensus() {
		const CONSENT_VALUE: u64 = 5;
		const WRONG_VALUE: u64 = 1;
		const NUMBER_OF_AUTHORITIES: u32 = 10;

		let mut electoral_system =
			MockElectoralSystem::<MonotonicMedian<u64, (), MockHook>>::new(0, (), ());

		let votes: Vec<((), u64)> = (1..=NUMBER_OF_AUTHORITIES)
			.map(|id| if id % 3 == 0 { ((), WRONG_VALUE) } else { ((), CONSENT_VALUE) })
			.collect();

		let consensus = electoral_system
			.new_election((), (), ())
			.unwrap()
			.check_consensus(None, votes, NUMBER_OF_AUTHORITIES)
			.unwrap();

		assert_eq!(consensus, Some(CONSENT_VALUE));
	}
}
