use super::core::{IndexOf, Indexed, Validate};

#[cfg(test)]
use proptest::prelude::{BoxedStrategy, Just, Strategy};
#[cfg(test)]
use proptest::test_runner::TestRunner;
#[cfg(test)]
use sp_std::fmt::Debug;

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
///  - `Output` and a function `step(&mut State, Input) -> Output`.
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
	type Settings;
	type Output: Validate;
	type State: Validate;
	// type DisplayState;

	/// To every state, this function associates a set of input indices which
	/// describes what kind of input(s) we want to receive next.
	fn input_index(s: &mut Self::State) -> IndexOf<Self::Input>;

	/// The state transition function, it takes the state, and an input,
	/// and assumes that both state and index are valid, and furthermore
	/// that the input has the index `input_index(s)`.
	fn step(s: &mut Self::State, i: Self::Input, set: &Self::Settings) -> Self::Output;

	/// Contains an optional specification of the `step` function.
	/// Takes a state, input and next state as arguments. During testing it is verified
	/// that the resulting state after the step function always fulfills this specification.
	#[cfg(test)]
	fn step_specification(
		_before: &mut Self::State,
		_input: &Self::Input,
		_settings: &Self::Settings,
		_after: &Self::State,
	) {
	}

	/// Given strategies `states` and `inputs` for generating arbitrary, valid values, runs the step
	/// function and ensures that it's result is always valid and additionally fulfills the
	/// `step_specification`.
	#[cfg(test)]
	fn test(
		path: &'static str,
		states: impl Strategy<Value = Self::State>,
		settings: impl Strategy<Value = Self::Settings>,
		inputs: impl Fn(IndexOf<Self::Input>) -> BoxedStrategy<Self::Input>,
	) where
		Self::State: sp_std::fmt::Debug + Clone + Send,
		Self::Input: sp_std::fmt::Debug + Clone + Send,
		Self::Settings: sp_std::fmt::Debug + Clone + Send,
		<Self::Input as Indexed>::Index: Ord,
	{
		use proptest::test_runner::{Config, FileFailurePersistence};

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
					(Just(state.clone()), inputs(Self::input_index(&mut state)), Just(settings))
				})),
				run_with_timeout(
					10,
					|(mut state, input, settings): (Self::State, Self::Input, Self::Settings)| {
						println!("running test");
						// ensure that inputs are well formed
						assert!(
							state.is_valid().is_ok(),
							"input state not valid {:?}",
							state.is_valid()
						);
						assert!(input.is_valid().is_ok(), "input not valid {:?}", input.is_valid());
						assert!(
							input.has_index(&Self::input_index(&mut state)),
							"input has wrong index"
						);

						// backup state
						let mut prev_state = state.clone();

						// run step function and ensure that output is valid
						assert!(
							Self::step(&mut state, input.clone(), &settings).is_valid().is_ok(),
							"step function failed"
						);

						// ensure that state is still well formed
						assert!(
							state.is_valid().is_ok(),
							"state after step function is not valid ({:?})",
							state
						);

						// ensure that step function computed valid state
						Self::step_specification(&mut prev_state, &input, &settings, &state);

						println!("done test");
						Ok(())
					},
				),
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
		let f1 = f.clone();
		let a1 = a.clone();
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			// .unhandled_panic(UnhandledPanic::ShutdownRuntime)
			.build()
			.unwrap()
			.block_on(async move {
				let f2 = f1.clone();
				let a2 = a1.clone();
				let a3 = a1.clone();
				tokio::time::timeout(
					std::time::Duration::from_secs(seconds),
					tokio::task::spawn_blocking(move || f2(a2)),
				)
				.await
				.map_err(move |_| format!("task failed with input {:?}", a3))
				.unwrap()
			})
			.unwrap()
	}
}
