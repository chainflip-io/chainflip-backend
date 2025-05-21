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

macro_rules! implementations {
	([$($Name:tt)*], [$($Parameters:tt)*], impl $Trait:tt { $($TraitDef:tt)* } $($rest:tt)* ) => {

		impl <$($Parameters)*> $Trait for $($Name)* {
			$($TraitDef)*
		}

		crate::electoral_systems::state_machine::core::implementations! {
			[$($Name)*], [$($Parameters)*], $($rest)*
		}
		
	};

	([$($Name:tt)*], [$($Parameters:tt)*],) => {}
}
pub(crate) use implementations;

macro_rules! defx {
	(
		pub $def:tt $Name:tt [$($ParamName:ident: $ParamType:tt),*] {
			$($Definition:tt)*
		} 
		validate $this:ident (else $Error:ident) {
			$($prop_name:ident : $prop:expr),*

			$(,
			( where
				$(
					$prop_var:ident = $prop_var_value:expr
				),*
			))?
		}
		$(
			with {
				$($Attributes:tt)*
			}
		)?;

		$($rest:tt)*
	) => {

		#[derive(Debug)]
		#[allow(non_camel_case_types)]
		pub enum $Error {
			$($prop_name),*
		}

		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize,
		)]

		$(
			$($Attributes)*
		)?
		pub $def $Name<$($ParamName: $ParamType),*> {
			$($Definition)*
		}

		impl<$($ParamName: $ParamType),*> Validate for $Name<$($ParamName),*> {

			type Error = $Error;

			fn is_valid(&self) -> Result<(), Self::Error> {
				let $this = self;

				$(
					$(
						let $prop_var = $prop_var_value;
					)*
				)?

				$(
					frame_support::ensure!($prop, $Error::$prop_name);
				)*
				Ok(())
			}
		}

		crate::electoral_systems::state_machine::core::implementations!{[$Name<$($ParamName),*>], [ $($ParamName: $ParamType),* ], $($rest)*}
	};
}
pub(crate) use defx; // <-- the trick


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

// Nice functions to have
pub fn fst<A, B>((a, _): (A, B)) -> A {
	a
}
pub fn snd<A, B>((_, b): (A, B)) -> B {
	b
}
