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

/// This electoral system calculates the median of all the authorities votes and stores the latest
/// median in the `ElectoralUnsynchronisedState`. Each time consensus is gained, everyone is asked
/// to revote, to provide a new updated value. *IMPORTANT*: This is not the most secure method as
/// only 1/3 is needed to change the median's value arbitrarily, even though we do use the same
/// median calculation elsewhere. For something more secure see `MonotonicMedian`.
///
/// `Settings` can be used by governance to provide information to authorities about exactly how
/// they should `vote`.
pub struct UnsafeMedian<Value, UnsynchronisedSettings, Settings> {
	_phantom: core::marker::PhantomData<(Value, UnsynchronisedSettings, Settings)>,
}
impl<
		Value: Member + Parameter + MaybeSerializeDeserialize + Ord,
		UnsynchronisedSettings: Member + Parameter + MaybeSerializeDeserialize,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
	> ElectoralSystem for UnsafeMedian<Value, UnsynchronisedSettings, Settings>
{
	type ElectoralUnsynchronisedState = Value;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = UnsynchronisedSettings;
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = ();
	type ElectionState = ();
	type Vote =
		vote_storage::individual::Individual<(), vote_storage::individual::shared::Shared<Value>>;
	type Consensus = Value;
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

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
			.map_err(|_| CorruptStorageError)?
		{
			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if let Some(consensus) = election_access.check_consensus()?.has_consensus() {
				election_access.delete();
				electoral_access.set_unsynchronised_state(consensus)?;
				electoral_access.new_election((), (), ())?;
			}
		} else {
			electoral_access.new_election((), (), ())?;
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		mut votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let votes_count = votes.len();
		Ok(
			if votes_count != 0 &&
				votes_count >= success_threshold_from_share_count(authorities) as usize
			{
				let (_, (_properties, median_vote), _) =
					votes.select_nth_unstable((votes_count - 1) / 2);
				Some(median_vote.clone())
			} else {
				None
			},
		)
	}
}

#[cfg(test)]
mod test_unsafe_median {

	use super::*;

	use crate::{
		electoral_system::{mocks::*, ConsensusStatus, ElectoralReadAccess},
		ElectionIdentifier, UniqueMonotonicIdentifier,
	};

	#[test]
	fn if_consensus_update_unsynchronised_state() {
		let election_identifier = ElectionIdentifier::new(UniqueMonotonicIdentifier::new(0), ());

		const INIT_UNSYNCHRONISED_STATE: u64 = 22;
		const NEW_UNSYNCHRONISED_STATE: u64 = 33;
		let mut electoral_access =
			MockElectoralAccess::<UnsafeMedian<u64, (), ()>>::new(INIT_UNSYNCHRONISED_STATE, ());

		electoral_access.set_consensus_status(
			election_identifier,
			ConsensusStatus::Changed {
				previous: INIT_UNSYNCHRONISED_STATE,
				new: NEW_UNSYNCHRONISED_STATE,
			},
		);

		UnsafeMedian::<u64, (), ()>::on_finalize(
			&mut electoral_access,
			vec![election_identifier],
			&(),
		)
		.unwrap();

		assert_eq!(electoral_access.unsynchronised_state().unwrap(), NEW_UNSYNCHRONISED_STATE);
	}

	#[test]
	fn if_no_consensus_do_not_update_unsynchronised_state() {
		let election_identifier = ElectionIdentifier::new(UniqueMonotonicIdentifier::new(0), ());

		const INIT_UNSYNCHRONISED_STATE: u64 = 22;
		let mut electoral_access =
			MockElectoralAccess::<UnsafeMedian<u64, (), ()>>::new(INIT_UNSYNCHRONISED_STATE, ());

		electoral_access.set_consensus_status(election_identifier, ConsensusStatus::None);

		UnsafeMedian::<u64, (), ()>::on_finalize(
			&mut electoral_access,
			vec![election_identifier],
			&(),
		)
		.unwrap();

		assert_eq!(electoral_access.unsynchronised_state().unwrap(), INIT_UNSYNCHRONISED_STATE);
	}

	#[test]
	fn check_consensus_correctly_calculates_median_when_all_authorities_vote() {
		let election_identifier = ElectionIdentifier::new(UniqueMonotonicIdentifier::new(0), ());

		const INIT_UNSYNCHRONISED_STATE: u64 = 22;
		let mut electoral_access =
			MockElectoralAccess::<UnsafeMedian<u64, (), ()>>::new(INIT_UNSYNCHRONISED_STATE, ());

		let mut votes = (1..=10).map(|v| ((), v)).collect::<Vec<_>>();

		use rand::{seq::SliceRandom, thread_rng};

		// vote ordering should not affect the result
		votes.shuffle(&mut thread_rng());

		let votes_len = votes.len() as u32;

		let consensus = UnsafeMedian::<u64, (), ()>::check_consensus(
			election_identifier,
			&electoral_access.election_mut(election_identifier).unwrap(),
			None,
			votes,
			// all authorities have voted
			votes_len,
		)
		.unwrap();

		assert_eq!(consensus, Some(5));
	}

	// Note: This is the reason the median is "unsafe" as 1/3 of validators can influence the value
	// in this case.
	#[test]
	fn check_consensus_correctly_calculates_median_when_exactly_super_majority_authorities_vote() {
		let election_identifier = ElectionIdentifier::new(UniqueMonotonicIdentifier::new(0), ());

		let mut electoral_access =
			MockElectoralAccess::<UnsafeMedian<u64, (), ()>>::new(Default::default(), ());

		let mut votes = vec![((), 1u64), ((), 5), ((), 3), ((), 2), ((), 8), ((), 6)];

		use rand::{seq::SliceRandom, thread_rng};

		// vote ordering shouldn't matter
		votes.shuffle(&mut thread_rng());

		let votes_len = votes.len() as u32;

		let consensus = UnsafeMedian::<u64, (), ()>::check_consensus(
			election_identifier,
			&electoral_access.election_mut(election_identifier).unwrap(),
			None,
			votes,
			(votes_len + (votes_len / 2)) as u32,
		)
		.unwrap();

		assert_eq!(consensus, Some(3));
	}

	#[test]
	fn fewer_than_supermajority_votes_does_not_get_consensus() {
		let election_identifier = ElectionIdentifier::new(UniqueMonotonicIdentifier::new(0), ());

		let mut electoral_access =
			MockElectoralAccess::<UnsafeMedian<u64, (), ()>>::new(Default::default(), ());

		let all_votes = vec![((), 1u64), ((), 5), ((), 3), ((), 2), ((), 8)];

		(0..(all_votes.len())).for_each(|n_votes| {
			assert_eq!(
				UnsafeMedian::<u64, (), ()>::check_consensus(
					election_identifier,
					&electoral_access.election_mut(election_identifier).unwrap(),
					None,
					all_votes.clone().into_iter().take(n_votes).collect::<Vec<_>>(),
					(all_votes.len() + (all_votes.len() / 2)) as u32,
				)
				.unwrap(),
				None
			);
		});
	}
}
