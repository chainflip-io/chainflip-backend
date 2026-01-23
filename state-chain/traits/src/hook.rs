
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, vec::Vec};

use crate::validate::Validate;

pub trait HookType {
	type Input;
	type Output;
}

impl<A,B> HookType for (A,B) {
	type Input = A;
	type Output = B;
}

pub trait Hook<T: HookType, const NAME: &'static str = "">: Validate {
	fn run(&mut self, input: T::Input) -> T::Output;
}

pub mod hook_test_utils {
	use super::*;
	use cf_utilities::impls;
use codec::MaxEncodedLen;
	#[cfg(test)]
	use proptest_derive::Arbitrary;

	#[derive(
		Clone,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		Serialize,
		Deserialize,
	)]
	#[cfg_attr(test, derive(Arbitrary))]
	#[serde(bound = "T::Input: Serialize + for<'d> Deserialize<'d>, WrappedHook: Serialize + for<'d> Deserialize<'d>")]
	pub struct MockHook<
		T: HookType,
		const NAME: &'static str = "",
		WrappedHook: Hook<T> = ConstantHook<T>,
	> {
		pub state: WrappedHook,
		pub call_history: Vec<T::Input>,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impls! {
		for MockHook<T, NAME, WrappedHook> where
		(
			T: HookType,
			const NAME: &'static str,
			WrappedHook: Hook<T>
		):

		impl {
			pub fn new(b: WrappedHook) -> Self {
				Self { state: b, call_history: Vec::new(), _phantom: Default::default() }
			}

			pub fn take_history(&mut self) -> Vec<T::Input> {
				sp_std::mem::take(&mut self.call_history)
			}
		}

		impl Validate {
			type Error = ();

			fn is_valid(&self) -> Result<(), ()> {
				Ok(())
			}
		}

		impl Default where (WrappedHook: Default)
		{
			fn default() -> Self {
				Self::new(Default::default())
			}
		}

		impl Hook<T> where (T::Input: Clone + Debug)
		{
			fn run(&mut self, input: T::Input) -> T::Output {
				#[cfg(test)]
				if !NAME.is_empty() {
					println!("{} called for {input:?}", NAME);
				}
				self.call_history.push(input.clone());
				self.state.run(input)
			}
		}
	}

	#[derive(
		Clone,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		Serialize,
		Deserialize,
	)]
	#[cfg_attr(test, derive(Arbitrary))]
	#[serde(bound = "T::Output: Serialize + for<'d> Deserialize<'d>")]
	pub struct ConstantHook<T: HookType> {
		pub state: T::Output,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impls! {
		for ConstantHook<T> where
		(
			T: HookType,
		):

		impl {
			pub fn new(b: T::Output) -> Self {
				Self { state: b, _phantom: Default::default() }
			}
		}

		impl Validate {
			type Error = ();

			fn is_valid(&self) -> Result<(), ()> {
				Ok(())
			}
		}

		impl Default where (T::Output: Default)
		{
			fn default() -> Self {
				Self::new(Default::default())
			}
		}

		impl Hook<T> where (T::Input: Debug, T::Output: Clone)
		{
			fn run(&mut self, _input: T::Input) -> T::Output {
				self.state.clone()
			}
		}
	}

	/// Hook to use for when we want to not do anything, for example
	/// useful for "disabling" debug hooks in production.
	/// It is marked as `inline` so shouldn't have any runtime cost.
	#[derive(
		Clone,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Debug,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		Serialize,
		Deserialize,
		Default,
	)]
	pub struct EmptyHook {}

	impl<X: HookType<Output = ()>> Hook<X> for EmptyHook {
		#[inline]
		fn run(&mut self, _input: <X as HookType>::Input) -> <X as HookType>::Output {}
	}

	impl Validate for EmptyHook {
		type Error = ();

		fn is_valid(&self) -> Result<(), Self::Error> {
			Ok(())
		}
	}
}