use crate::electoral_system::ElectoralSystem;





pub trait Fibered {
    type Base;
    fn is_in_fiber(&self, base: &Self::Base) -> bool;
}

pub trait Pointed {
    fn pt() -> Self;
}

pub trait Validate {
    type Error : sp_std::fmt::Debug;
    fn is_valid(&self) -> Result<(), Self::Error>;
}

pub mod dependent_state_machine {
    use super::{Fibered, Validate};

    pub trait Parameter : 'static {
        type State: Fibered<Base = bool>;
        type Input: Fibered;
        type Output;
    }

    pub struct Phantom<P: Parameter> {
        _phantom: core::marker::PhantomData<P>
    }

    pub trait Trait : 'static {
        type Input: Validate + Fibered;
        type Output: Validate;
        type State : Validate;
        type DisplayState;

        fn request(s: &Self::State) -> <Self::Input as Fibered>::Base;
        fn step(s: &mut Self::State, i: Self::Input) -> Self::Output;
        fn get(s: &Self::State) -> Self::DisplayState;
    }

}

pub trait DependentStateMachineTrait {
    type State: Fibered<Base = bool>;
    type Input: Fibered;
    type Output;

    fn input(s: Self::State) -> <Self::Input as Fibered>::Base;
    fn step(s: Self::State, i: Self::Input) -> (Self::State, Self::Output);
}

pub trait DependentStateMachineParams {
}

pub struct DependentStateMachine<'a, State, Input: Fibered + Pointed, Output> {
    pub initial: &'a fn() -> State,
    pub request: &'a fn(State) -> Input::Base,
    pub step: &'a fn(State, Input) -> (State, Output)
}


