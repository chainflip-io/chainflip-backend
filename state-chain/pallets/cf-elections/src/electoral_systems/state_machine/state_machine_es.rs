use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use itertools::Either;
use sp_std::{fmt::Debug, vec::Vec};

use crate::{
	electoral_system::{ElectionReadAccess, ElectionWriteAccess, ElectoralSystem},
	vote_storage::VoteStorage,
	CorruptStorageError,
};

use super::{
	consensus::{ConsensusMechanism, Threshold},
	core::{Indexed, MultiIndexAndValue, Validate},
	state_machine::StateMachine,
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

impl<V: Validate, C: Validate> Validate for SMInput<V, C> {
	type Error = Either<V::Error, C::Error>;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			SMInput::Vote(vote) => vote.is_valid().map_err(Either::Left),
			SMInput::Context(context) => context.is_valid().map_err(Either::Right),
		}
	}
}

pub trait ESInterface {
	type ValidatorId: Parameter + Member + MaybeSerializeDeserialize;

	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedState: Parameter + Member + MaybeSerializeDeserialize;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedStateMapKey: Parameter + Member;
	/// This is intended for storing any internal state of the ElectoralSystem. It is not
	/// synchronised and therefore should only be used by the ElectoralSystem, and not be consumed
	/// by the engine.
	///
	/// Also note that if this state is changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedStateMapValue: Parameter + Member;

	/// Settings of the electoral system. These can be changed at any time by governance, and
	/// are not synchronised with elections, and therefore there is not a universal mapping from
	/// elections to these settings values. Therefore it should only be used for internal
	/// state, i.e. the engines should not consume this data.
	///
	/// Also note that if these settings are changed that will not cause election's consensus to be
	/// retested.
	type ElectoralUnsynchronisedSettings: Parameter + Member + MaybeSerializeDeserialize;

	/// Settings of the electoral system. These settings are synchronised with
	/// elections, so all engines will have a consistent view of the electoral settings to use for a
	/// given election.
	type ElectoralSettings: Parameter + Member + MaybeSerializeDeserialize + Eq;

	/// Extra data stored along with the UniqueMonotonicIdentifier as part of the
	/// ElectionIdentifier. This is used by composite electoral systems to identify which variant of
	/// election it is working with, without needing to reading in further election
	/// state/properties/etc.
	type ElectionIdentifierExtra: Parameter + Member + Copy + Eq + Ord;

	/// The properties of a single election, for example this could describe which block of the
	/// external chain the election is associated with and what needs to be witnessed.
	type ElectionProperties: Parameter + Member;

	/// Per-election state needed by the ElectoralSystem. This state is not synchronised across
	/// engines, and may change during the lifetime of a election.
	type ElectionState: Parameter + Member;

	/// A description of the validator's view of the election's topic. For example a list of all
	/// ingresses the validator has observed in the block the election is about.
	type Vote: VoteStorage;

	/// This is the information that results from consensus. Typically this will be the same as the
	/// `Vote` type, but with more complex consensus models the result of an election may not be
	/// sensibly represented in the same form as a single vote.
	type Consensus: Parameter + Member + Eq;

	/// Custom parameters for `on_finalize`. Used to communicate information like the latest chain
	/// tracking block to the electoral system. While it gives more flexibility to use a generic
	/// type here, instead of an associated type, particularly as it would allow `on_finalize` to
	/// take trait instead of a specific type, I want to avoid spreading additional generics
	/// throughout the rest of the code. As an alternative, you can use dynamic dispatch (i.e.
	/// Box<dyn ...>) to achieve much the same affect.
	type OnFinalizeContext;

	/// Custom return of the `on_finalize` callback. This can be used to communicate any information
	/// you want to the caller.
	type OnFinalizeReturn;
}

#[allow(type_alias_bounds)]
type VoteOfVoteStorage<VS: VoteStorage> = VS::Vote;

pub trait StateMachineES:
	'static
	+ Sized
	+ ESInterface<
		ElectoralUnsynchronisedStateMapKey = (),
		ElectoralUnsynchronisedStateMapValue = (),
		ElectoralSettings = (),
		ElectionIdentifierExtra = (),
		ElectionState = (),
		OnFinalizeContext = Vec<Self::OnFinalizeContextItem>,
		OnFinalizeReturn = Vec<Self::OnFinalizeReturnItem>,
		Consensus = Self::Consensus2,
		Vote = Self::VoteStorage2,
	>
{
	type OnFinalizeContextItem: Clone + Debug;
	type OnFinalizeReturnItem;

	type Consensus2: Indexed<Index = Vec<Self::ElectionProperties>> + Parameter + Member + Eq;
	type Vote2: Validate + Indexed<Index = Vec<Self::ElectionProperties>> + Parameter + Member + Eq;
	type VoteStorage2: VoteStorage<Vote = Self::Vote2>;

	type StateMachine: StateMachineForES<Self> + 'static;
	type ConsensusMechanism: ConsensusMechanismForES<Self> + 'static;
}

pub trait StateMachineForES<ES: StateMachineES> = StateMachine<
	Input = SMInput<
		MultiIndexAndValue<ES::ElectionProperties, ES::Consensus>,
		ES::OnFinalizeContextItem,
	>,
	State = ES::ElectoralUnsynchronisedState,
	Settings = ES::ElectoralUnsynchronisedSettings,
	Output = Result<ES::OnFinalizeReturnItem, &'static str>,
>;

pub trait ConsensusMechanismForES<ES: StateMachineES> = ConsensusMechanism<
	Vote = VoteOfVoteStorage<ES::Vote>,
	Result = ES::Consensus,
	Settings = (Threshold, ES::ElectionProperties),
>;

pub struct StateMachineESInstance<Bounds: StateMachineES> {
	_phantom: core::marker::PhantomData<Bounds>,
}

impl<Bounds: StateMachineES> ElectoralSystem for StateMachineESInstance<Bounds>
where
	<Bounds::Vote as VoteStorage>::Properties: Default,
	Bounds::Consensus: Indexed,
{
	type ValidatorId = Bounds::ValidatorId;
	type ElectoralUnsynchronisedState = Bounds::ElectoralUnsynchronisedState;
	type ElectoralUnsynchronisedStateMapKey = Bounds::ElectoralUnsynchronisedStateMapKey;
	type ElectoralUnsynchronisedStateMapValue = Bounds::ElectoralUnsynchronisedStateMapValue;
	type ElectoralUnsynchronisedSettings = Bounds::ElectoralUnsynchronisedSettings;
	type ElectoralSettings = Bounds::ElectoralSettings;
	type ElectionIdentifierExtra = Bounds::ElectionIdentifierExtra;
	type ElectionProperties = Bounds::ElectionProperties;
	type ElectionState = Bounds::ElectionState;
	type Vote = Bounds::Vote;
	type Consensus = Bounds::Consensus;
	type OnFinalizeContext = Bounds::OnFinalizeContext;
	type OnFinalizeReturn = Bounds::OnFinalizeReturn;

	fn generate_vote_properties(
		_election_identifier: crate::electoral_system::ElectionIdentifierOf<Self>,
		_previous_vote: Option<(
			crate::electoral_system::VotePropertiesOf<Self>,
			crate::electoral_system::AuthorityVoteOf<Self>,
		)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<crate::electoral_system::VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(Default::default())
	}

	fn on_finalize<
		ElectoralAccess: crate::electoral_system::ElectoralWriteAccess<ElectoralSystem = Self> + 'static,
	>(
		election_identifiers: Vec<crate::electoral_system::ElectionIdentifierOf<Self>>,
		contexts: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		// initialize the result value
		let mut result = Vec::new();

		// read state
		log::debug!("ESSM: reading state & settings");
		let mut state = ElectoralAccess::unsynchronised_state()?;
		let settings = ElectoralAccess::unsynchronised_settings()?;

		// define step function which progresses the state machine
		// by one input
		let mut step = |input| {
			Bounds::StateMachine::step(&mut state, input, &settings)
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
				step(SMInput::Vote(MultiIndexAndValue(election_access.properties()?, input)))?;
			}
		}

		// gather the input indices after all state transitions
		let input_indices: Vec<_> = Bounds::StateMachine::input_index(&state).into_iter().collect();
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

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: crate::electoral_system::ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		log::debug!("ESSM consensus: reading properties");
		let properties = election_access.properties()?;
		log::debug!("ESSM consensus: reading properties done");
		let mut consensus = Bounds::ConsensusMechanism::default();
		let num_authorities = consensus_votes.num_authorities();

		let mut properties_vec = Vec::new();
		properties_vec.push(properties.clone());

		for vote in consensus_votes.active_votes() {
			// insert vote if it is valid for the given properties
			if vote.is_valid().is_ok() && vote.has_index(&properties_vec) {
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
