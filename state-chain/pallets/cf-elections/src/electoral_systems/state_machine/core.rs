use codec::{Decode, Encode};
use derive_where::derive_where;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, vec::Vec};

/// Syntax sugar for implementing multiple traits for a single type.
///
/// Example use:
///
/// impls! {
///     for u8:
///     Clone {
///         ...
///     }
///     Copy {
///         ...
///     }
///     Default {
///         ...
///     }
/// }
macro_rules! impls {
    (for $name:ty $(where ($($bounds:tt)*))? :
	$(#[doc = $doc_text:tt])? impl $($trait:ty)?  $(where ($($trait_bounds:tt)*))? {$($trait_impl:tt)*}
	$($rest:tt)*
	) => {
        $(#[doc = $doc_text])?
        impl$(<$($bounds)*>)? $($trait for)? $name
		$(where $($trait_bounds)*)?
		{
            $($trait_impl)*
        }
        impls!{for $name $(where ($($bounds)*))? : $($rest)*}
    };
    (for $name:ty $(where ($($bounds:tt)*))? :) => {}
}

/// Type which can be used for implementing traits that
/// contain only type definitions, as used in many parts of
/// the state machine based electoral systems.
#[derive_where(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord;)]
#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
#[codec(encode_bound())]
#[serde(bound = "")]
#[scale_info(skip_type_params(Tag))]
pub(crate) struct TypesFor<Tag> {
	_phantom: sp_std::marker::PhantomData<Tag>,
}

pub trait HookType {
	type Input;
	type Output;
}

pub trait Hook<T: HookType> {
	fn run(&mut self, input: T::Input) -> T::Output;
}

pub mod hook_test_utils {
	use super::*;
	use codec::MaxEncodedLen;

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
	#[serde(bound = "T::Input: Serde, T::Output: Serde")]
	pub struct MockHook<T: HookType, const NAME: &'static str = ""> {
		pub state: T::Output,
		pub call_history: Vec<T::Input>,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impls! {
		for MockHook<T, NAME> where
		(
			T: HookType,
			const NAME: &'static str,
		):

		impl {
			pub fn new(b: T::Output) -> Self {
				Self { state: b, call_history: Vec::new(), _phantom: Default::default() }
			}

			pub fn take_history(&mut self) -> Vec<T::Input> {
				sp_std::mem::take(&mut self.call_history)
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
			fn run(&mut self, input: T::Input) -> T::Output {
				#[cfg(test)]
				if !NAME.is_empty() {
					println!("{} called for {input:?}", NAME);
				}
				self.call_history.push(input);
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
		fn run(&mut self, _input: <X as HookType>::Input) -> <X as HookType>::Output {
			()
		}
	}
}

/// Dedicated `Validate` trait for cases where a value
/// has to be validate with respect to an index, as is
/// the case for the input type of state machines.
pub trait IndexedValidate<Index, Value> {
	type Error;
	fn validate(index: &Index, value: &Value) -> Result<(), Self::Error>;
}

/// A type which can be validated.
pub trait Validate {
	type Error: sp_std::fmt::Debug;
	fn is_valid(&self) -> Result<(), Self::Error>;
}

impl Validate for () {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<A, B: sp_std::fmt::Debug + Clone> Validate for Result<A, B> {
	type Error = B;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			Ok(_) => Ok(()),
			Err(err) => Err(err.clone()),
		}
	}
}

/// Encapsulating usual constraints on types meant to be serialized
pub trait Serde = Serialize + for<'a> Deserialize<'a>;
