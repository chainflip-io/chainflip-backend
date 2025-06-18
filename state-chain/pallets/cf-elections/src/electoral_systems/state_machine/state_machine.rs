use super::core::Validate;
use derive_where::derive_where;
use itertools::Either;
use sp_std::{fmt::Debug, vec::Vec};

#[cfg(test)]
use proptest::prelude::{BoxedStrategy, Just, Strategy};
#[cfg(test)]
use proptest::test_runner::TestRunner;

pub trait AbstractApi {
	type Query;
	type Response;
	type Error;

	fn validate(query: &Self::Query, response: &Self::Response) -> Result<(), Self::Error>;
}

/// Custom error type for validation of `SMInput`.
#[derive_where(Debug; Error: Debug, Context::Error: Debug)]
pub enum SMInputValidateError<Context: Validate, Error> {
	WrongIndex,
	InvalidConsensus(Error),
	InvalidContext(Context::Error),
}

/// A trait for implementing state machines, in particular used for electoral systems.
///
/// See also the documentation for `StatemachineElectoralSystem`.
///
/// An electoral system is essentialy a state machine: it keeps track of an internal state,
/// processes votes as input, and produces a result in every `on_finalize` call.
///
/// Thus the basic structure is that we have three associated types:
///  - `State`
///  - `Input`
///  - `Output` and a function `step(&mut State, Input) -> Output`.
///
/// NOTE: Due to ergonomic reasons, we have another associated type `Settings` which has the same
/// purpose as `Input`, but which is passed by reference to the `step` function, instead of by
/// value. It would be possible to merge these into a new struct `{input: Input, settings: &'a
/// Settings}`, and thus do away with the `Settings` associated type, but it seems to be more
/// ergonomic to keep it, as long as we only use the state machine abstraction for electoral system
/// which always have a settings component.
///
/// ## Mapping to elections
/// For an electoral system, the `Input` type is always of the form `SMInput<(Properties,
/// Consensus),Context>`, which is an `Either` type carrying either a tuple consisting of election
/// properties and consensus or alternatively the type of the `OnFinalizeContext`.
///
/// The `InputIndex` type is a vector of election properties, the function `input_index()` computes
/// to each state the properties of all elections that should be open for this state. This means
/// that creation of elections is handled indirectly: The state machine merely has to transition
/// into a state with the correct `input_index`, all elections with these election properties is
/// going to be created automatically, and potentially ongoing elections which are not in the
/// `input_index` are going to be deleted.
///
/// The `step` function takes as input the current state, settings, and an input, and computes the
/// next state, as well as a result. The input is either the consensus gained in some election which
/// was open, or alternatively the `OnFinalizeContext` which is being passed to this ES from another
/// ES upstream.   
///
/// ## Multiple elections
/// The `step` function is designed as the smallest logical state transition of the ES. Thus it
/// takes as input only a single consensus *or* a single context. Thus, during a typical
/// `on_finalize` call, the `step` function is called multiple times, once for each available input.
/// The outputs of each `step` call are collected into a vector and passed to the next ES.
///
/// ## Validation
/// It is common to have certain validity requirements which one assumes to hold for either
/// the input, state, or output. These are encoded by `Validate` bounds
/// on the `Output` and `State` type, as well as an `IndexedValidateFor<InputIndex,Input>` bound on
/// `Self`. The latter allows a tuple of (InputIndex, Input) to be checked for validity: i.e., given
/// an input index, and an input, we not only check that the input is well formed, but we also
/// ensure that the input is valid *for a given input index*.
///
/// For example, in the X, elections are created to witness block headers starting from a given
/// height, in that case we can ensure that the received vector of block headers actually starts
/// with the correct height.
///
/// Note: The `IndexedValidateFor` bound is somewhat strangely on `Self`, and not directly on either
/// `Input` or `InputIndex` because this makes it possible to automatically derive an implementation
/// for inputs of the form `SMInput<_,_>` from a simpler implementation of
/// `IndexedValidateFor<ElectionProperties, Consensus>` for any ES.
///
/// ## Testing
/// The state machine trait provides a convenience method `test(states, settings, inputs)` for
/// testing a given state machine. Here `states`, `settings` and `inputs` are strategies for
/// generating states, settings and inputs, and the function runs the `step` function on randomly
/// generated input values, while ensuring that all inputs and outputs are valid as per .
///
/// Additionally the `step_specification` function can be implemented, in order to provide custom
/// pre-/postconditions to be checked during `test()`.
pub trait Statemachine: AbstractApi + 'static {
	type Context: Validate;
	type Settings;
	type Output: Validate;
	type State: Validate;

	/// To every state, this function associates a set of input indices which
	/// describes what kind of input(s) we want to receive next.
	fn input_index(s: &mut Self::State) -> Vec<Self::Query>;

	fn validate_input(
		index: &[Self::Query],
		value: &InputOf<Self>,
	) -> Result<(), SMInputValidateError<Self::Context, Self::Error>>
	where
		Self::Query: PartialEq,
	{
		match value {
			Either::Right((property, consensus)) =>
				if index.contains(property) {
					Self::validate(property, consensus)
						.map_err(SMInputValidateError::InvalidConsensus)
				} else {
					Err(SMInputValidateError::WrongIndex)
				},

			Either::Left(context) =>
				context.is_valid().map_err(SMInputValidateError::InvalidContext),
		}
	}

	/// The state transition function, it takes the state, and an input,
	/// and assumes that both state and index are valid, and furthermore
	/// that the input has the index `input_index(s)`.
	fn step(
		state: &mut Self::State,
		input: InputOf<Self>,
		settings: &Self::Settings,
	) -> Self::Output;

	/// Contains an optional specification of the `step` function.
	/// Takes a state, input and next state as arguments. During testing it is verified
	/// that the resulting state after the step function always fulfills this specification.
	#[cfg(test)]
	fn step_specification(
		_before: &mut Self::State,
		_input: &InputOf<Self>,
		_output: &Self::Output,
		_settings: &Self::Settings,
		_after: &Self::State,
	) {
	}

	/// Runs the step function and validates the input and state both before and after, as well as
	/// the output. This does *not* check that the input is valid for the given state because if
	/// the SM is run as part of an electoral system this might not always be the case.
	#[cfg(test)]
	fn step_and_validate(
		mut state: &mut Self::State,
		input: InputOf<Self>,
		settings: &Self::Settings,
	) -> Self::Output
	where
		Self::Query: sp_std::fmt::Debug + Clone + Send + PartialEq,
		Self::Response: sp_std::fmt::Debug + Clone + Send,
		Self::State: sp_std::fmt::Debug + Clone + Send,
		Self::Context: sp_std::fmt::Debug + Clone + Send,
		Self::Settings: sp_std::fmt::Debug + Clone + Send,
		Self::Error: sp_std::fmt::Debug,
	{
		// ensure that inputs are well formed
		assert!(state.is_valid().is_ok(), "input state not valid {:?}", state.is_valid());
		// backup state
		let mut prev_state = state.clone();

		// run step function and ensure that output is valid
		let output = Self::step(&mut state, input.clone(), &settings);
		assert!(output.is_valid().is_ok(), "step function failed");

		// ensure that state is still well formed
		assert!(
			state.is_valid().is_ok(),
			"state after step function is not valid ({:#?}), reason: {:?}",
			state,
			state.is_valid()
		);

		// ensure that step function computed valid state
		Self::step_specification(&mut prev_state, &input, &output, &settings, &state);

		output
	}

	/// Given strategies `states` and `inputs` for generating arbitrary, valid values, runs the step
	/// function and ensures that it's result is always valid and additionally fulfills the
	/// `step_specification`.
	#[cfg(test)]
	fn test(
		path: &'static str,
		states: impl Strategy<Value = Self::State>,
		settings: impl Strategy<Value = Self::Settings>,
		inputs: impl Fn(Self::Query) -> BoxedStrategy<Self::Response>,
		context: impl Fn(&Self::State) -> BoxedStrategy<Self::Context>,
	) where
		Self::Query: sp_std::fmt::Debug + Clone + Send + PartialEq,
		Self::Response: sp_std::fmt::Debug + Clone + Send,
		Self::State: sp_std::fmt::Debug + Clone + Send,
		Self::Context: sp_std::fmt::Debug + Clone + Send,
		Self::Settings: sp_std::fmt::Debug + Clone + Send,
		Self::Error: sp_std::fmt::Debug,
	{
		use proptest::{
			prop_oneof,
			sample::select,
			test_runner::{Config, FileFailurePersistence},
		};

		let mut runner = TestRunner::new(Config {
			source_file: Some(path),
			failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
				"proptest-regressions",
			))),
			cases: 256 * 16, // 256 is the default
			..Default::default()
		});

		runner
			.run(
				&((states, settings).prop_flat_map(|(mut state, settings)| {
					(
						Just(state.clone()),
						{
							// If there are no input indices we don't want to take
							// the branch that selects an input index. This code is
							// a bit convoluted, but it was the most convenient way
							// to select between two different strategies.
							let weight =
								if Self::input_index(&mut state).is_empty() { 0 } else { 1 };
							prop_oneof![
								1 => context(&state).prop_map(Either::Left),
								weight => select(Self::input_index(&mut state))
									.prop_flat_map(|index| (Just(index.clone()), inputs(index)))
									.prop_map(Either::Right),
							]
						},
						Just(settings),
					)
				})),
				#[allow(clippy::type_complexity)]
				// run_with_timeout(
				// 	500,
				|(mut state, input, settings): (
					Self::State,
					Either<Self::Context, (Self::Query, Self::Response)>,
					Self::Settings,
				)| {
					// ensure input has correct index
					Self::validate_input(&Self::input_index(&mut state), &input)
						.unwrap_or_else(|_| panic!("input has wrong index: {input:?}"));

					// run step and verify all other properties
					let _output = Self::step_and_validate(&mut state, input, &settings);

					Ok(())
				},
				// ),
			)
			.unwrap();
	}
}

#[cfg(test)]
pub fn run_with_timeout<
	A: Send + Clone + Debug + 'static,
	B: Send + 'static,
	F: Fn(A) -> B + Send + Clone + 'static,
>(
	seconds: u64,
	f: F,
) -> impl Fn(A) -> B {
	move |a| {
		let (sender, receiver) = std::sync::mpsc::channel();
		let a1 = a.clone();
		let f1 = f.clone();
		std::thread::spawn(move || {
			let result = f1(a1);
			sender.send(result).unwrap();
		});

		receiver
			.recv_timeout(std::time::Duration::from_secs(seconds))
			.unwrap_or_else(|_| panic!("task failed due to timeout with input {a:#?}"))
	}
}

pub type InputOf<X> =
	Either<<X as Statemachine>::Context, (<X as AbstractApi>::Query, <X as AbstractApi>::Response)>;
