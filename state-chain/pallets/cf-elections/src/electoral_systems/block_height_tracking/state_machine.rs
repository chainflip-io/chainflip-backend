use crate::electoral_system::ElectoralSystem;

#[cfg(test)]
use proptest::prelude::{BoxedStrategy, Just, Strategy};
#[cfg(test)]
use proptest::test_runner::TestRunner;

pub trait Indexed {
	type Index;
	fn has_index(&self, base: &Self::Index) -> bool;
}

pub type IndexOf<Ixd> = <Ixd as Indexed>::Index;

pub trait Pointed {
	fn pt() -> Self;
}

pub trait Validate {
	type Error: sp_std::fmt::Debug;
	fn is_valid(&self) -> Result<(), Self::Error>;
}

pub trait DependentStateMachine: 'static {
	type Input: Validate + Indexed;
	type Output: Validate;
	type State: Validate;
	type DisplayState;

	fn input_index(s: &Self::State) -> IndexOf<Self::Input>;
	fn step(s: &mut Self::State, i: Self::Input) -> Self::Output;
	fn get(s: &Self::State) -> Self::DisplayState;

	fn step_specification(before: &Self::State, input: &Self::Input, after: &Self::State) -> bool {
		true
	}

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
