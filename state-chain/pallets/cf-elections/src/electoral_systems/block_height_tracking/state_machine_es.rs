use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::{Either, Itertools};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

use crate::{
	electoral_system::{ElectionReadAccess, ElectionWriteAccess, ElectoralSystem},
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

/// This is an Either type, unfortunately it's more ergonomic
/// to recreate this instead of using `itertools::Either`, because
/// we need a special implementation of Indexed: we want the vote to
/// be indexed but not the context.
#[derive(Debug, Clone, PartialEq)]
pub enum SMInput<Vote, Context> {
	Vote(Vote),
	Context(Context),
}

impl<V: Indexed, C> Indexed for SMInput<V, C> {
	type Index = V::Index;

	fn has_index(&self, index: &Self::Index) -> bool {
		match self {
			SMInput::Vote(vote) => vote.has_index(index),
			SMInput::Context(_) => true,
		}
	}
}

impl<V: Validate, C> Validate for SMInput<V, C> {
	type Error = V::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			SMInput::Vote(vote) => vote.is_valid(),
			SMInput::Context(_) => Ok(()),
		}
	}
}

/// Creates an Electoral System from a given state machine
/// and a given consensus mechanism.
pub struct DsmElectoralSystem<
	Type,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings,
	Context,
	Consensus,
> {
	_phantom: core::marker::PhantomData<(Type, ValidatorId, Settings, Context, Consensus)>,
}

impl<SM, ValidatorId, Settings, Context, C> ElectoralSystem
	for DsmElectoralSystem<SM, ValidatorId, Settings, Context, C>
where
	SM: StateMachine<
		Input = SMInput<<C as ConsensusMechanism>::Result, Context>,
		Settings = Settings,
	>,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
	Context: 'static + Clone + sp_std::fmt::Debug,
	C: ConsensusMechanism<Settings = (Threshold, <SM::Input as Indexed>::Index)> + 'static,
	<C as ConsensusMechanism>::Result: Indexed + Clone + Member + Parameter,
	<C as ConsensusMechanism>::Vote: Member
		+ Parameter
		+ Clone
		+ Validate
		+ Indexed<Index = <<C as ConsensusMechanism>::Result as Indexed>::Index>,
	<SM::Input as Indexed>::Index: Clone + Member + Parameter + sp_std::fmt::Debug,
	SM::State: MaybeSerializeDeserialize + Member + Parameter + Eq + sp_std::fmt::Debug,
	// SM::Input: Indexed + Clone + Member + Parameter,
	SM::Output: IntoResult,
	<SM::Output as IntoResult>::Err: sp_std::fmt::Debug,
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = SM::State;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = Settings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = <SM::Input as Indexed>::Index;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<<C as ConsensusMechanism>::Vote>;
	type Consensus = <C as ConsensusMechanism>::Result;
	type OnFinalizeContext = Vec<Context>;

	// we return either the state if no input was processed,
	// or the output produced by the state machine
	type OnFinalizeReturn = Vec<<SM::Output as IntoResult>::Ok>;

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
		contexts: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, crate::CorruptStorageError> {
		// initialize the result value
		let mut result = Vec::new();

		// read state
		log::debug!("ESSM: reading state & settings");
		let mut state = ElectoralAccess::unsynchronised_state()?;
		let settings = ElectoralAccess::unsynchronised_settings()?;

		// define step function which progresses the state machine
		// by one input
		let mut step = |input| {
			SM::step(&mut state, input, &settings)
				.into_result()
				.map(|output| {
					result.push(output);
					()
				})
				.map_err(|err| {
					log::error!("Electoral system moved into a bad state: {err:?}");
					CorruptStorageError::new()
				})
		};

		// step with OnFinalizeContext
		log::debug!("ESSM: stepping for each context (n = {:?})", contexts.len());
		for context in contexts {
			log::debug!("ESSM: stepping with context {context:?}");
			step(SMInput::Context(context.clone()))?;
		}

		// step for each election that reached consensus
		log::debug!("ESSM: stepping for each election with consensus ({:?})", election_identifiers);
		for election_identifier in &election_identifiers {
			let election_access = ElectoralAccess::election_mut(election_identifier.clone());
			log::debug!("ESSM: checking consensus for {election_identifier:?}");
			if let Some(input) = election_access.check_consensus()?.has_consensus() {
				log::debug!("ESSM: stepping with input {input:?}");
				step(SMInput::Vote(input))?;
			}
		}

		// gather the input indices after all state transitions
		let input_indices = SM::input_index(&state);
		let mut open_elections = Vec::new();

		// delete elections which are no longer in the input indices
		// NOTE: This happens after *all* step functions have been run
		// (thus cannot be part of the loop above) since we first want to
		// apply *all* state transitions to determine which elections should
		// be kept open.
		for election_identifier in election_identifiers {
			let election = ElectoralAccess::election_mut(election_identifier);
			log::debug!("ESSM: getting properties");
			let properties = election.properties()?;
			if !input_indices.contains(&properties) {
				log::info!("deleting election for {properties:?}");
				election.delete();
			} else {
				log::info!("keeping election for {properties:?}");
				open_elections.push(properties.clone());
			}
		}

		// Create elections for new input indices which weren't open before,
		// i.e. contained in `input_indices` but not in `open_elections`.
		for index in input_indices.iter().filter(|index| !open_elections.contains(index)) {
			log::info!("creating election for {index:?}");
			ElectoralAccess::new_election((), index.clone(), ())?;
		}

		log::debug!("ESSM: setting state");
		ElectoralAccess::set_unsynchronised_state(state)?;

		return Ok(result);

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
		log::debug!("ESSM consensus: reading properties");
		let properties = election_access.properties()?;
		log::debug!("ESSM consensus: reading properties done");
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

		log::debug!("ESSM consensus: calling consensus mechanism");
		Ok(consensus.check_consensus(&(
			Threshold { threshold: success_threshold_from_share_count(num_authorities) },
			properties,
		)))
	}
}
