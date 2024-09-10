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
