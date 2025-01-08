
use core::{iter::Step, result};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::MaxEncodedLen;

pub trait Hook<A,B> {
	fn run(&self, input: A) -> B;
}

#[cfg(test)]
pub mod hook_test_utils {
    use super::*;

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
    pub struct ConstantHook<A,B> {
        pub state: B,
        pub _phantom: sp_std::marker::PhantomData<A>
    }

    impl<A,B> ConstantHook<A,B> {
        pub fn new(b: B) -> Self {
            Self { state: b, _phantom: Default::default() }
        }
    }

    impl<A,B: Clone> Hook<A,B> for ConstantHook<A,B> {
        fn run(&self, input: A) -> B {
            self.state.clone()
        }
    }
}


pub trait SaturatingStep : Step + Clone {
    fn saturating_forward(start: Self, mut count: usize) -> Self {
        for _ in 0..count {
            if let Some(result) =  Self::forward_checked(start.clone(), count) {
                return result;
            } else {
                count /= 2;
            }
        }
        return start;
    }
}

impl<X: Step + Clone> SaturatingStep for X {}