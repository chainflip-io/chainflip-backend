use codec::{Decode, Encode};
use itertools::Either;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

pub trait Hook<A, B> {
	fn run(&self, input: A) -> B;
}

#[cfg(test)]
pub mod hook_test_utils {
	use super::*;
	use codec::MaxEncodedLen;

	#[derive(
		Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	pub struct ConstantHook<A, B> {
		pub state: B,
		pub _phantom: sp_std::marker::PhantomData<A>,
	}

	impl<A, B> ConstantHook<A, B> {
		pub fn new(b: B) -> Self {
			Self { state: b, _phantom: Default::default() }
		}
	}

	impl<A, B: Default> Default for ConstantHook<A, B> {
		fn default() -> Self {
			Self::new(Default::default())
		}
	}

	impl<A, B: Clone> Hook<A, B> for ConstantHook<A, B> {
		fn run(&self, _input: A) -> B {
			self.state.clone()
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
	pub _phantom: sp_std::marker::PhantomData<Idx>,
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
