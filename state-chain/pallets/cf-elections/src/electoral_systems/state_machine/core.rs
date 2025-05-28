use core::ops::RangeInclusive;

use cf_chains::{witness_period::BlockWitnessRange, ChainWitnessConfig};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque};

use codec::{Decode, Encode};
use derive_where::derive_where;
use itertools::Either;
use scale_info::TypeInfo;
use scale_info::prelude::string::String;
use serde::{Deserialize, Serialize};
use sp_core::H256;
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

/// Adds the type parameters to all given implementatios
macro_rules! implementations {
	([$($Name:tt)*], [$($Parameters:tt)*], impl { $($Implementation:tt)* } $($rest:tt)* ) => {

		impl <$($Parameters)*> $($Name)* {
			$($Implementation)*
		}

		crate::electoral_systems::state_machine::core::implementations! {
			[$($Name)*], [$($Parameters)*], $($rest)*
		}
	};

	([$($Name:tt)*], [$($Parameters:tt)*], impl$(<$($TraitParamName:ident: $TraitParamPath:path),*>)? $Trait:path { $($TraitDef:tt)* } $($rest:tt)* ) => {

		impl <$($Parameters)*, $($($TraitParamName: $TraitParamPath),*)?> $Trait for $($Name)* {
			$($TraitDef)*
		}

		crate::electoral_systems::state_machine::core::implementations! {
			[$($Name)*], [$($Parameters)*], $($rest)*
		}

	};

	([$($Name:tt)*], [$($Parameters:tt)*],) => {}
}
pub(crate) use implementations;

/// Derive error enum cases from a struct or enum definition
macro_rules! derive_error_enum {
	($Error:ident [$($ParamsDef:tt)*], struct { $( $(#[doc = $doc_text:tt])* $vis:vis $Field:ident: $Type:ty, )* } { $( $property:ident ),* }
	) => {

		#[derive_where::derive_where(Debug, PartialEq)]
		#[allow(non_camel_case_types)]
		pub enum $Error<$($ParamsDef)*> {

			$(
				$Field(<$Type as Validate>::Error),
			)*

			$(
				$property,
			)*
		}

	};

	($Error:ident [$($ParamName:ident: $ParamType:tt),*], enum { $( $anything:tt )* } { $( $property:ident ),* }
	) => {

		#[derive_where::derive_where(Debug, PartialEq; )]
		#[allow(non_camel_case_types)]
		pub enum $Error<$($ParamName: $ParamType),*> {

			// $(
			// 	$Field(<$Type as Validate>::Error),
			// )*

			$(
				$property,
			)*

			PhantomCase(sp_std::marker::PhantomData<($($ParamName,)*)>)
		}

	};
}
pub(crate) use derive_error_enum;

macro_rules! derive_validation_statements {
	($this:ident, $Error:ident, struct { $( $(#[doc = $doc_text:tt])* $vis:vis $Field:ident: $Type:ty, )* }
	) => {
		$(
			$this.$Field.is_valid().map_err($Error::$Field)?;
		)*
	};

	($Error:ident, $this:ident, enum { $( $anything:tt )* }
	) => {
	};
}
pub(crate) use derive_validation_statements;

macro_rules! def_derive {
	(#[no_serde] $($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo,
		)]
		$($Definition)*
	};
	($($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo,
		)]
		#[derive(Deserialize, Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub(crate) use def_derive;

/// Syntax sugar for adding validation code to types with validity requirements
macro_rules! defx {
	(
		$(#[$($Attributes:tt)*])*
		pub $def:tt $Name:tt [$($ParamName:ident $(: $ParamType:tt)?),*] {
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

		$($rest:tt)*
	) => {

		crate::electoral_systems::state_machine::core::derive_error_enum!{$Error [ $($ParamName: $($ParamType)?),* ], $def { $($Definition)* } { $($prop_name),* } }


		crate::electoral_systems::state_machine::core::def_derive!{
			$(
				#[$($Attributes)*]
			)*
			pub $def $Name<$($ParamName: $($ParamType)?),*> {
				$($Definition)*
			}
		}

		impl<$($ParamName: $($ParamType)?),*> Validate for $Name<$($ParamName),*> {

			type Error = $Error<$($ParamName),*>;

			fn is_valid(&self) -> Result<(), Self::Error> {
				let $this = self;

				$(
					$(
						let $prop_var = $prop_var_value;
					)*
				)?

				crate::electoral_systems::state_machine::core::derive_validation_statements!($this, $Error, $def { $($Definition)* } );

				$(
					frame_support::ensure!($prop, $Error::$prop_name);
				)*
				Ok(())
			}
		}

		crate::electoral_systems::state_machine::core::implementations!{[$Name<$($ParamName),*>], [ $($ParamName: $($ParamType)?),* ], $($rest)*}
	};
}
pub(crate) use defx;

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

impl<Tag> Validate for TypesFor<Tag> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

pub trait HookType {
	type Input;
	type Output;
}

pub trait Hook<T: HookType>: Validate {
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
		fn run(&mut self, _input: <X as HookType>::Input) -> <X as HookType>::Output {}
	}

	impl Validate for EmptyHook {
		type Error = ();

		fn is_valid(&self) -> Result<(), Self::Error> {
			Ok(())
		}
	}
}

/// A type which can be validated.
pub trait Validate {
	type Error: sp_std::fmt::Debug + PartialEq;
	fn is_valid(&self) -> Result<(), Self::Error>;
}

impl Validate for () {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T> Validate for sp_std::marker::PhantomData<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<A: Validate, B: Validate> Validate for BTreeMap<A, B> {
	type Error = Either<A::Error, B::Error>;

	fn is_valid(&self) -> Result<(), Self::Error> {
		for (k, v) in self {
			k.is_valid().map_err(Either::Left)?;
			v.is_valid().map_err(Either::Right)?;
		}
		Ok(())
	}
}

impl<A: Validate> Validate for BTreeSet<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.iter().try_for_each(Validate::is_valid)
	}
}

#[cfg(test)]
impl Validate for String {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<A: Validate> Validate for Vec<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.iter().try_for_each(Validate::is_valid)
	}
}

impl<A: Validate> Validate for VecDeque<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.iter().try_for_each(Validate::is_valid)
	}
}

impl<A: Validate> Validate for Option<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.as_ref().map(Validate::is_valid).unwrap_or(Ok(()))
	}
}

impl<A: Validate> Validate for RangeInclusive<A> {
	type Error = A::Error;

	fn is_valid(&self) -> Result<(), Self::Error> {
		self.start().is_valid()?;
		self.end().is_valid()?;
		Ok(())
	}
}

impl<A, B: sp_std::fmt::Debug + Clone + PartialEq> Validate for Result<A, B> {
	type Error = B;

	fn is_valid(&self) -> Result<(), Self::Error> {
		match self {
			Ok(_) => Ok(()),
			Err(err) => Err(err.clone()),
		}
	}
}

impl<C: ChainWitnessConfig> Validate for BlockWitnessRange<C> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		// TODO, actually check something
		Ok(())
	}
}

impl Validate for bool {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for u8 {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for u16 {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for u32 {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for u64 {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for usize {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for char {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl Validate for H256 {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

/// Encapsulating usual constraints on types meant to be serialized
pub trait Serde = Serialize + for<'a> Deserialize<'a>;
