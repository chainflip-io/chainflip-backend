pub use codec::{Decode, Encode};
pub use enum_iterator::Sequence;
use frame_support::Parameter;
pub use scale_info::TypeInfo;
pub use serde::{Deserialize, Serialize};
pub use sp_std::fmt::Debug;

#[cfg(test)]
pub use proptest::prelude::Arbitrary;

/// Encapsulating usual constraints on types meant to be serialized
pub trait Serde = Serialize + for<'a> Deserialize<'a>;

#[cfg(test)]
pub trait TestTraits = Send + Sync;
#[cfg(not(test))]
pub trait TestTraits = core::any::Any;

#[cfg(test)]
pub trait MaybeArbitrary = proptest::prelude::Arbitrary + Send + Sync
where <Self as Arbitrary>::Strategy: Clone + Sync + Send;
#[cfg(not(test))]
pub trait MaybeArbitrary = core::any::Any;

pub trait CommonTraits = Parameter + Serde + 'static + Send + Sync;

//-------- derive macros ----------
#[cfg(test)]
pub use proptest_derive::Arbitrary;
