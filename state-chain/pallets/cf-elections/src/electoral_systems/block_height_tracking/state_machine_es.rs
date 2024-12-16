use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

use crate::{
	electoral_system::{ElectionWriteAccess, ElectoralSystem},
	vote_storage, CorruptStorageError,
};

use super::{
	consensus::{Consensus, Threshold},
	state_machine::{DependentStateMachine, Indexed, Validate},
};

pub struct DsmElectoralSystem<
	Type,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings,
	Consensus,
> {
	_phantom: core::marker::PhantomData<(Type, ValidatorId, Settings, Consensus)>,
}

pub enum Either<A, B> {
	Left(A),
	Right(B),
}

use Either::{Left, Right};

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

// -- deriving an electoral system from a statemachine
impl<DSM, ValidatorId, Settings, C> ElectoralSystem
	for DsmElectoralSystem<DSM, ValidatorId, Settings, C>
where
	DSM: DependentStateMachine,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
	C: Consensus<
			Vote = DSM::Input,
			Result = DSM::Input,
			Settings = (Threshold, <DSM::Input as Indexed>::Index),
		> + 'static,
	<DSM::Input as Indexed>::Index: Clone + Member + Parameter + sp_std::fmt::Debug,
	DSM::State: MaybeSerializeDeserialize + Member + Parameter + Eq + sp_std::fmt::Debug,
	DSM::Input: Indexed + Clone + Member + Parameter,
	DSM::Output: IntoResult,
	<DSM::Output as IntoResult>::Err: sp_std::fmt::Debug,
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = DSM::State;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = ();
	type ElectionProperties = <DSM::Input as Indexed>::Index;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<DSM::Input>;
	type Consensus = DSM::Input;
	type OnFinalizeContext = ();

	// we return either the state if no input was processed,
	// or the output produced by the state machine
	type OnFinalizeReturn = Either<DSM::DisplayState, <DSM::Output as IntoResult>::Ok>;

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

			if let Some(input) = election_access.check_consensus()?.has_consensus() {
				let (input_request, output) =
					ElectoralAccess::mutate_unsynchronised_state(|state| {
						let output = DSM::step(state, input);

						match output.into_result() {
							Ok(output) => Ok((DSM::input_index(state), output)),
							Err(err) => {
								log::error!("Electoral system moved into a bad state: {err:?}");
								Err(CorruptStorageError::new())
							},
						}
					})?;

				// delete the old election and create a new one with the new input request
				election_access.delete();
				ElectoralAccess::new_election((), input_request, ())?;

				Ok(Right(output))
			} else {
				log::info!("No consensus could be reached!");

				Ok(Left(DSM::get(&ElectoralAccess::unsynchronised_state()?)))
			}
		} else {
			log::info!("Starting new election with initial value because no elections exist");

			let state = ElectoralAccess::unsynchronised_state()?;

			ElectoralAccess::new_election((), DSM::input_index(&state), ())?;
			Ok(Left(DSM::get(&state)))
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
