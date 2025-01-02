
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

    impl<A,B: Clone> Hook<A,B> for ConstantHook<A,B> {
        fn run(&self, input: A) -> B {
            self.state.clone()
        }
    }
}
