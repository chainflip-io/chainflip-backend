use codec::{Decode, Encode};
use derive_where::derive_where;
use itertools::Either;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{fmt::Debug, vec::Vec};

/// Syntax sugar for implementing multiple traits for a single type.
///
/// Example use:
/// ```
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
/// ```
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
	pub struct ConstantHook<T: HookType> {
		pub state: T::Output,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impl<T: HookType> ConstantHook<T> {
		pub fn new(b: T::Output) -> Self {
			Self { state: b, _phantom: Default::default() }
		}
	}

	impl<T: HookType> Default for ConstantHook<T>
	where
		T::Output: Default,
	{
		fn default() -> Self {
			Self::new(Default::default())
		}
	}

	impl<T: HookType> Hook<T> for ConstantHook<T>
	where
		T::Output: Clone,
	{
		fn run(&mut self, _input: T::Input) -> T::Output {
			self.state.clone()
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
	pub struct IncreasingHook<T: HookType> {
		pub counter: u32,
		pub state: T::Output,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impl<T: HookType> IncreasingHook<T> {
		pub fn new(counter_value: u32, state: T::Output) -> Self {
			Self { counter: counter_value, state, _phantom: Default::default() }
		}
	}

	impl<T: HookType> Default for IncreasingHook<T>
	where
		T::Output: Default,
	{
		fn default() -> Self {
			Self::new(Default::default(), Default::default())
		}
	}

	impl<T: HookType> Hook<T> for IncreasingHook<T>
	where
		T::Output: Clone,
	{
		fn run(&mut self, _input: T::Input) -> T::Output {
			self.counter += 1;
			self.state.clone()
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
	#[serde(bound = "T::Input: Serde, T::Output: Serde")]
	pub struct MockHook<const NAME: &'static str, T: HookType> {
		pub state: T::Output,
		pub call_history: Vec<T::Input>,
		pub _phantom: sp_std::marker::PhantomData<T>,
	}

	impls! {
		for MockHook<NAME,T> where
		(
			const NAME: &'static str,
			T: HookType
		):

		impl {
			pub fn new(b: T::Output) -> Self {
				Self { state: b, call_history: Vec::new(), _phantom: Default::default() }
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
				println!("{} called for {input:?}", NAME);
				self.call_history.push(input);
				self.state.clone()
			}
		}
	}
}

/// A type which has an associated index type.
/// This effectively models types families.
pub trait Indexed {
	type Index;
	fn has_index(&self, index: &Self::Index) -> bool;
}

pub type IndexOf<Ixd> = <Ixd as Indexed>::Index;

//--- instances ---
impl<A: Indexed, B: Indexed<Index = A::Index>> Indexed for Either<A, B> {
	type Index = A::Index;

	fn has_index(&self, index: &Self::Index) -> bool {
		match self {
			Either::Left(a) => a.has_index(index),
			Either::Right(b) => b.has_index(index),
		}
	}
}

impl<A: Indexed, B: Indexed<Index = A::Index>> Indexed for (A, B) {
	type Index = A::Index;

	fn has_index(&self, index: &Self::Index) -> bool {
		self.0.has_index(index) && self.1.has_index(index)
	}
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct ConstantIndex<Idx, A> {
	pub data: A,
	_phantom: sp_std::marker::PhantomData<Idx>,
}
impl<Idx, A> ConstantIndex<Idx, A> {
	pub fn new(data: A) -> Self {
		ConstantIndex { data, _phantom: Default::default() }
	}
}
impl<Idx, A> Indexed for ConstantIndex<Idx, A> {
	type Index = Vec<Idx>;

	fn has_index(&self, _index: &Self::Index) -> bool {
		true
	}
}
impl<Idx, A> Validate for ConstantIndex<Idx, A> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Ord, PartialOrd,
)]
pub struct MultiIndexAndValue<Idx, A>(pub Idx, pub A);

impl<Idx: PartialEq, A> Indexed for MultiIndexAndValue<Idx, A> {
	type Index = Vec<Idx>;

	fn has_index(&self, indices: &Self::Index) -> bool {
		indices.contains(&self.0)
	}
}

impl<Idx, A> Validate for MultiIndexAndValue<Idx, A> {
	type Error = &'static str;

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
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
