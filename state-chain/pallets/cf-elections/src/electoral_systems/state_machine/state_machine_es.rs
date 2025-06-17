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
	CorruptStorageError, ElectoralSystemTypes, PartialVoteOf,
};

use super::{
	consensus::{ConsensusMechanism, SuccessThreshold},
	state_machine::{AbstractApi, Statemachine},
};

/// Main trait for deriving an electoral system from a state machine and consensus mechanism.
/// It ensures that all the associated types match up as required. See the documentation for
/// `StatemachineElectoralSystem` for more information on how to implement it.
pub trait StatemachineElectoralSystemTypes: 'static + Sized {
	type ValidatorId: Parameter + Member;
	type StateChainBlockNumber: Parameter + Member + Ord;

	type OnFinalizeReturnItem;
	type VoteStorage: VoteStorage;

	type Statemachine: StatemachineForES<Self> + 'static;
	type ConsensusMechanism: ConsensusMechanismForES<Self::Statemachine> + 'static;
}

/// Convenience wrapper of the `Statemachine` trait. Given an electoral system `ES`,
/// this trait defines the conditions on the state machine's associated types for it
/// to be possible to derive an electoral system.
pub trait StatemachineForES<ES: StatemachineElectoralSystemTypes> = Statemachine<
	Output = Result<ES::OnFinalizeReturnItem, &'static str>,
	Response = <ES::VoteStorage as VoteStorage>::Vote,
>;

/// Convenience wrapper of the `ConsensusMechanism` trait. Given an electoral system `ES`,
/// this trait defines the conditions on the consensus mechanism's associated types for it
/// to be possible to derive an electoral system.
pub trait ConsensusMechanismForES<S: Statemachine> = ConsensusMechanism<
	Vote = S::Response,
	Result = S::Response,
	Settings = (SuccessThreshold, S::Query),
>;

/// ### Electoral system derived from a state machine.
///
/// In order to derive such an electoral system (ES), the following steps have to be taken:
///
/// 1. Create a new "tag type" for the ES, whose purpose it is to carry the implementation of
///    various traits and their associated types. This type is only relevant at compile time and
///    should not contain any runtime data. You'll need to implement various common traits for it
///    (`Debug`, `Clone`, `Serialize`, `TypeInfo`, `Encode`, etc.), which you can either do by hand,
///    or use a convenience wrapper such as `TypesFor<_>`. Let's call this tag type `ES`.
///
/// 2. Implement the traits `ElectoralSystemTypes` and `StatemachineElectoralSystemTypes` for your
///    tag type. As you can see in the definition, the latter trait depends on the former, but with
///    various conditions on the associated types. This means that you are not totally free in
///    choosing all the associated types, but have to follow these conditions. For instance, both
///    `OnFinalizeContext` and `OnFinalizeReturn` have to be of type `Vec<_>`.
///
/// 3. You'll find that in the previous step you have to choose associated types `Statemachine` and
///    `ConsensusMechanism`. If you want to implement a new state machine and/or a new consensus
///    mechanism, you'll have to do either of the following (ES refers to the tag type you created
///    in step 1):
///     - Create a new tag type for the state machine, and implement the trait
///       `StatemachineForES<ES>`.
///     - Create a new tag type for the consensus mechanism, and implement the trait
///       `ConsensusMechanismForES<ES>`.
///
///    The conditions on associated types in the definitions of `StatemachineForES` and
///    `ConsensusMechanismForES` ensure that all components fit together into a coherent electoral
///    system.
///
/// 4. Enjoy your new electoral system! It is called `StatemachineElectoralSystem<ES>`, where `ES`
///    is the tag type from step 1.
pub struct StatemachineElectoralSystem<ES: StatemachineElectoralSystemTypes> {
	_phantom: core::marker::PhantomData<ES>,
}

impl<ES: StatemachineElectoralSystemTypes> ElectoralSystemTypes for StatemachineElectoralSystem<ES>
where
	<ES::Statemachine as Statemachine>::State: Parameter + Member + MaybeSerializeDeserialize,
	<ES::Statemachine as Statemachine>::Settings: Parameter + Member + MaybeSerializeDeserialize,
	<ES::Statemachine as AbstractApi>::Query: Parameter + Member,
	<ES::Statemachine as AbstractApi>::Response: Parameter + Member + Eq,
{
	type ValidatorId = ES::ValidatorId;
	type StateChainBlockNumber = ES::StateChainBlockNumber;
	type ElectoralUnsynchronisedState = <ES::Statemachine as Statemachine>::State;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();
	type ElectoralUnsynchronisedSettings = <ES::Statemachine as Statemachine>::Settings;
	type ElectoralSettings = ();
	type ElectionIdentifierExtra = ();
	type ElectionProperties = <ES::Statemachine as AbstractApi>::Query;
	type ElectionState = ();
	type VoteStorage = ES::VoteStorage;
	type Consensus = <ES::Statemachine as AbstractApi>::Response;
	type OnFinalizeContext = Vec<<ES::Statemachine as Statemachine>::Context>;
	type OnFinalizeReturn = Vec<ES::OnFinalizeReturnItem>;
}

/// ### Implementation of the `ElectoralSystem` trait for a given state machine and consensus mechanism.
///
/// See the documentation of `StatemachineForElectoralSystem`
/// for more information on how to assemble all components correctly.
///
/// This ES behaves as follows:
///  - `check_consensus` is delegated to the given consensus mechanism. Votes which are invalid
///    according to the `IsValid` implementation on the `ES::Vote` type are ignored.
///  - The state machine's state is stored in `ElectoralUnsynchronizedState`.
///  - During `on_finalize`, the state machine's step function is called once for each value in the
///    `OnFinalizeContext` vector, and after that, once for each election that has come to
///    consensus.
///  - The outputs of each individual `step` call are collected into a vector, in order to be
///    returned as the final `OnFinalizeReturn` result.
///  - After completing all `step` calls, the `input_index` function of the SM is called. Its result
///    is a vector of election properties. The intention is that elections for exactly these
///    properties should be active after the `on_finalize` terminates. Thus, the ES deletes all
///    elections whose corresponding election properties are not in the `input_index()` vector, and
///    creates new elections for all properties in `input_index()`, which aren't currently
///    associated with an election.
///  - Finally the state machine's new state is written back into `ElectoralUnsynchronizedState`.
///
/// WARNING:
/// Due to the fact that elections are created and deleted based solely on the `input_index()`
/// vector of election properties, this means that it might be tricky to implement "refreshing" of
/// elections in cases where the election properties don't change.
///  - It is sensible to follow the `ElectionIdentifierExtra` approach of the ES trait, and add a
///    separate counter to the election properties which can be incremented if a new election with
///    the same properties is required.
///  - Due to the fact that in a single `on_finalize` call, the `step` function might be run
///    multiple times, extra care needs to be taken to ensure that this produces the appropriate
///    behaviour of refreshing elections. For example, it might be the case that the `step` function
///    is called twice, and in the first call removes property `A` from its list of input indices,
///    and in the second call adds property `A` back again. The expected behaviour might be that the
///    election for `A` is recreated, but the current observed behaviour will be that the election
///    for `A` simply stays open - because the `input_index()` function is only called after both
///    step transitions, and thus doesn't notice that `A` got removed and added back.
///  - In the currently implemented ESs this is not a problem, but has to be checked when new state
///    machines are designed.
impl<ES: StatemachineElectoralSystemTypes> ElectoralSystem for StatemachineElectoralSystem<ES>
where
	<ES::VoteStorage as VoteStorage>::Properties: Default,
	<ES::Statemachine as Statemachine>::State: Parameter + Member + MaybeSerializeDeserialize,
	<ES::Statemachine as Statemachine>::Settings: Parameter + Member + MaybeSerializeDeserialize,
	<ES::Statemachine as AbstractApi>::Query: Parameter + Member,
	<ES::Statemachine as AbstractApi>::Response: Parameter + Member + Eq,

	<ES::Statemachine as Statemachine>::Context: Debug + Clone,
{
	fn generate_vote_properties(
		_election_identifier: crate::electoral_system::ElectionIdentifierOf<Self>,
		_previous_vote: Option<(
			crate::electoral_system::VotePropertiesOf<Self>,
			crate::electoral_system::AuthorityVoteOf<Self>,
		)>,
		_vote: &PartialVoteOf<Self>,
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

		let mut state = ElectoralAccess::unsynchronised_state()?;
		let settings = ElectoralAccess::unsynchronised_settings()?;

		// define step function which progresses the state machine
		// by one input
		let mut step = |input| {
			ES::Statemachine::step(&mut state, input, &settings)
				.map(|output| {
					result.push(output);
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
			step(Either::Left(context.clone()))?;
		}

		// step for each election that reached consensus
		log::info!("ESSM: stepping for each election with consensus ({:?})", election_identifiers);
		for election_identifier in &election_identifiers {
			let election_access = ElectoralAccess::election_mut(*election_identifier);
			log::debug!("ESSM: checking consensus for {election_identifier:?}");
			if let Some(input) = election_access.check_consensus()?.has_consensus() {
				log::debug!("ESSM: stepping with input {input:?}");
				step(Either::Right((election_access.properties()?, input)))?;
			}
		}

		// gather the input indices after all state transitions
		let input_indices: Vec<_> = ES::Statemachine::input_index(&mut state);
		let mut open_elections = Vec::new();

		// delete elections which are no longer in the input indices
		// NOTE: This happens after *all* step functions have been run
		// (thus cannot be part of the loop above) since we first want to
		// apply *all* state transitions to determine which elections should
		// be kept open.
		for election_identifier in election_identifiers {
			let election = ElectoralAccess::election_mut(election_identifier);
			let properties: <ES::Statemachine as AbstractApi>::Query = election.properties()?;
			if !input_indices.contains(&properties) {
				election.delete();
			} else {
				open_elections.push(properties);
			}
		}

		// Create elections for new input indices which weren't open before,
		// i.e. contained in `input_indices` but not in `open_elections`.
		for index in input_indices.into_iter().filter(|index| !open_elections.contains(index)) {
			ElectoralAccess::new_election((), index, ())?;
		}

		ElectoralAccess::set_unsynchronised_state(state)?;

		Ok(result)
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		// This is the consensus as of the last time the consensus was checked. Note this is *NOT*
		// the "last" consensus, i.e. this can be `None` even if on some previous check we had
		// consensus, but it was subsequently lost.
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: crate::electoral_system::ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let properties = election_access.properties()?;
		let mut consensus = ES::ConsensusMechanism::default();
		let num_authorities = consensus_votes.num_authorities();

		for vote in consensus_votes.active_votes() {
			// insert vote if it is valid for the given properties
			if ES::Statemachine::validate(&properties, &vote).is_ok() {
				consensus.insert_vote(vote);
			} else {
				log::warn!("Received invalid vote: expected base {properties:?} but vote was not in fiber ({:?})", vote);
			}
		}

		Ok(consensus.check_consensus(&(
			SuccessThreshold {
				success_threshold: success_threshold_from_share_count(num_authorities),
			},
			properties,
		)))
	}
}
