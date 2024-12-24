use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::{Either, Itertools};
use sp_std::vec::Vec;

use crate::{
	electoral_system::{ElectionWriteAccess, ElectoralSystem},
	vote_storage, CorruptStorageError,
};

use super::{
	consensus::{ConsensusMechanism, Threshold},
	state_machine::{Indexed, StateMachine, Validate},
};

pub trait IntoResult {
	type Ok;
	type Err;

	fn into_result(self) -> Result<Self::Ok, Self::Err>;
}

impl<A, B> IntoResult for Result<A, B> {
	type Ok = A;
	type Err = B;
	fn into_result(self) -> Result<A, B> {
		self
	}
}

/// Creates an Electoral System from a given state machine
/// and a given consensus mechanism.
pub struct DsmElectoralSystem<
	Type,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings,
	Consensus,
> {
	_phantom: core::marker::PhantomData<(Type, ValidatorId, Settings, Consensus)>,
}

impl<SM, ValidatorId, Settings, C> ElectoralSystem
	for DsmElectoralSystem<SM, ValidatorId, Settings, C>
where
	SM: StateMachine,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
	C: ConsensusMechanism<
			Vote = SM::Input,
			Result = SM::Input,
			Settings = (Threshold, <SM::Input as Indexed>::Index),
		> + 'static,
	<SM::Input as Indexed>::Index: Clone + Member + Parameter + sp_std::fmt::Debug,
	SM::State: MaybeSerializeDeserialize + Member + Parameter + Eq + sp_std::fmt::Debug,
	SM::Input: Indexed + Clone + Member + Parameter,
	SM::Output: IntoResult,
	<SM::Output as IntoResult>::Err: sp_std::fmt::Debug,
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = SM::State;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = <SM::Input as Indexed>::Index;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<SM::Input>;
	type Consensus = SM::Input;
	type OnFinalizeContext = ();

	// we return either the state if no input was processed,
	// or the output produced by the state machine
	type OnFinalizeReturn = Either<SM::DisplayState, <SM::Output as IntoResult>::Ok>;

	fn generate_vote_properties(
		_election_identifier: crate::electoral_system::ElectionIdentifierOf<Self>,
		_previous_vote: Option<(
			crate::electoral_system::VotePropertiesOf<Self>,
			crate::electoral_system::AuthorityVoteOf<Self>,
		)>,
		_vote: &<Self::Vote as crate::vote_storage::VoteStorage>::PartialVote,
	) -> Result<crate::electoral_system::VotePropertiesOf<Self>, crate::CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<
		ElectoralAccess: crate::electoral_system::ElectoralWriteAccess<ElectoralSystem = Self> + 'static,
	>(
		election_identifiers: Vec<crate::electoral_system::ElectionIdentifierOf<Self>>,
		context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, crate::CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let election_access = ElectoralAccess::election_mut(election_identifier);

			// if we have consensus, we can pass it to the state machine's step function
			if let Some(input) = election_access.check_consensus()?.has_consensus() {
				let (next_input_index, output) =
					ElectoralAccess::mutate_unsynchronised_state(|state| {
						// call the state machine
						let output = SM::step(state, input);

						// if we have been successful, get the input index of the new state
						match output.into_result() {
							Ok(output) => Ok((SM::input_index(state), output)),
							Err(err) => {
								log::error!("Electoral system moved into a bad state: {err:?}");
								Err(CorruptStorageError::new())
							},
						}
					})?;

				// delete the old election and create a new one with the new input index
				election_access.delete();
				ElectoralAccess::new_election((), next_input_index, ())?;

				Ok(Either::Right(output))
			} else {
				// if there is no consensus, simply get the current `DisplayState` of the SM.

				log::info!("No consensus could be reached!");
				Ok(Either::Left(SM::get(&ElectoralAccess::unsynchronised_state()?)))
			}
		} else {
			// if there is no election going on, we create an election corresponding to the
			// current state.

			log::info!("Starting new election with value because no elections exist");

			let state = ElectoralAccess::unsynchronised_state()?;

			ElectoralAccess::new_election((), SM::input_index(&state), ())?;
			Ok(Either::Left(SM::get(&state)))
		}
	}

	fn check_consensus<
		ElectionAccess: crate::electoral_system::ElectionReadAccess<ElectoralSystem = Self>,
	>(
		election_access: &ElectionAccess,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: crate::electoral_system::ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, crate::CorruptStorageError> {
		let properties = election_access.properties()?;
		let mut consensus = C::default();
		let num_authorities = consensus_votes.num_authorities();

		for vote in consensus_votes.active_votes() {
			// insert vote if it is valid for the given properties
			if vote.is_valid().is_ok() && vote.has_index(&properties) {
				log::info!("inserting vote {vote:?}");
				consensus.insert_vote(vote);
			} else {
				log::warn!("Received invalid vote: expected base {properties:?} but vote was not in fiber ({:?})", vote);
			}
		}

		Ok(consensus.check_consensus(&(
			Threshold { threshold: success_threshold_from_share_count(num_authorities) },
			properties,
		)))
	}
}
