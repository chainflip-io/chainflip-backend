use crate::electoral_system::ElectoralSystem;

#[cfg(test)]
use proptest::prelude::{BoxedStrategy, Just, Strategy};
#[cfg(test)]
use proptest::test_runner::TestRunner;

/// A type which has an associated index type.
/// This effectively models dependent sum types.
pub trait Indexed {
	type Index;
	fn has_index(&self, base: &Self::Index) -> bool;
}

pub type IndexOf<Ixd> = <Ixd as Indexed>::Index;

/// A type which can be validated.
pub trait Validate {
	type Error: sp_std::fmt::Debug;
	fn is_valid(&self) -> Result<(), Self::Error>;
}

/// A trait for implementing state machines, in particular used for simple electoral systems.
/// The model currently only supports electoral systems with a single ongoing election at any given
/// time. (Extending it to multiple ongoing elections is WIP.)
///
/// An electoral system is essentialy a state machine: it keeps track of an internal state,
/// processes votes as input, and produces a result in every `on_finalize` call.
///
/// Thus the basic structure is that we have three associated types:
///  - `State`
///  - `Input`
///  - `Output`
/// and a function `step(&mut State, Input) -> Output`.
///
/// ## Mapping to elections
/// The `Input` type is the type of votes. Election properties are given by the associated type
/// `Input::Index`, where the function `has_index(vote: &Input, election_properties: &Input::Index)
/// -> bool` is used to determine whether a given vote is valid for given election properties.
///
/// The definition of the state machine requires a function `input_index(&State) -> Input::Index`
/// which describes for a given state, which index we expect the next input to have (in other words,
/// for which election properties we want to get a vote next). This means that creation of elections
/// is handled indirectly: The state machine merely has to transition into a state with the correct
/// `input_index`, an election with these election properties is going to be created automatically.
///
/// ## Idle results
/// When there is no consensus, the electoral system still has to return sth in its `on_finalize`
/// function. This value is provided by the `get(&State) -> DisplayState` function. The associated
/// `DisplayState` type is an arbitrary "summary" of the current state, meant for consumers of the
/// `on_finalize` result.
///
/// Note: it might be that this functionality is going to be modelled differently in the future.
///
/// ## Validation
/// In the case of the BHW, both the `Input`, as well as the `State` contain sequences of headers
/// which need to have sequential block heights and matching hashes. In order to provide a coherent
/// interface for checking these, we require theses associated types to implement the trait
/// `Validate`. We also require `Validate` on the `Output` type.
///
/// ## Testing
/// The state machine trait provides a convenience method `test(states, inputs)` for testing a given
/// state machine. Here `states` and `inputs` are strategies for generating states and inputs, and
/// the function runs the `step` function on randomly generated input values, while ensuring that
/// everything is valid.
pub trait StateMachine: 'static {
	type Input: Validate + Indexed;
	type Output: Validate;
	type State: Validate;
	type DisplayState;

	/// To every state, this function associates an input index which
	/// describes what kind of input we want to receive next.
	fn input_index(s: &Self::State) -> IndexOf<Self::Input>;

	/// The state transition function, it takes the state, and an input,
	/// and assumes that both state and index are valid, and furthermore
	/// that the input has the index `input_index(s)`.
	fn step(s: &mut Self::State, i: Self::Input) -> Self::Output;

	/// Project the current state to a "DisplayState" value.
	fn get(s: &Self::State) -> Self::DisplayState;

	/// Contains an optional specification of the `step` function.
	/// Takes a state, input and next state as arguments. During testing it is verified
	/// that the resulting state after the step function always fulfills this specification.
	#[cfg(test)]
	fn step_specification(before: &Self::State, input: &Self::Input, after: &Self::State) -> bool {
		true
	}

	/// Given strategies `states` and `inputs` for generating arbitrary, valid values, runs the step
	/// function and ensures that it's result is always valid and additionally fulfills the
	/// `step_specification`.
	#[cfg(test)]
	fn test(
		states: impl Strategy<Value = Self::State>,
		inputs: impl Fn(IndexOf<Self::Input>) -> BoxedStrategy<Self::Input>,
	) where
		Self::State: sp_std::fmt::Debug + Clone,
		Self::Input: sp_std::fmt::Debug + Clone,
	{
		let mut runner = TestRunner::default();

		runner
			.run(
				&(states.prop_flat_map(|state| {
					(Just(state.clone()), inputs(Self::input_index(&state)))
				})),
				|(mut state, input)| {
					// ensure that inputs are well formed
					assert!(state.is_valid().is_ok(), "input state not valid");
					assert!(input.is_valid().is_ok(), "input not valid");
					assert!(input.has_index(&Self::input_index(&state)), "input has wrong index");

					// backup state
					let prev_state = state.clone();

					// run step function and ensure that output is valid
					assert!(
						Self::step(&mut state, input.clone()).is_valid().is_ok(),
						"step function failed"
					);

					// ensure that state is still well formed
					assert!(state.is_valid().is_ok(), "state after step function is not valid");
					assert!(
						Self::step_specification(&prev_state, &input, &state),
						"step function does not fulfill spec"
					);

					Ok(())
				},
			)
			.unwrap();
	}
}
